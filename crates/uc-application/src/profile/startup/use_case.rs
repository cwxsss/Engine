use std::sync::Arc;

use super::{
    ProfileStartupError, ProfileStartupStoragePort, ProfileUpgradeBackupError,
    ProfileUpgradeBackupPort, ProfileUpgradeVersions,
};
use crate::profile::factory_reset::{
    ProfileGeneration, ProfileLifecycle, ProfileLifecycleRepositoryPort, ProfileLifecycleState,
};

/// 升级前备份失败时，启动流程的取舍。
///
/// 备份是"升级出错还能回滚"的安全网，**不是启动的前置条件**。历史实现在备份失败时
/// 一律 fail-closed（拒绝启动），在真实平台上会造成**永久砖机**，且应用内没有任何
/// 恢复入口：
/// - HarmonyOS 应用沙箱按 MAC 策略拒绝 `link(2)`，归档发布必然 EACCES；
/// - 桌面端 secure storage 首次迁移会改写源文件，使既有备份记录的 digest 永久失配。
///
/// 因此把取舍交给宿主显式选择：默认保持 [`Self::FailClosed`]（不改变库的历史语义与
/// 单测），宿主在确知自己平台的沙箱限制时改用 [`Self::WarnAndContinue`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProfileUpgradeBackupPolicy {
    /// 备份失败即中止启动。
    #[default]
    FailClosed,
    /// 备份失败只记录告警，按"没有备份"继续启动。
    WarnAndContinue,
}

impl ProfileUpgradeBackupPolicy {
    const fn continues_after_failure(self) -> bool {
        matches!(self, Self::WarnAndContinue)
    }
}

/// 唯一升级前准备负责人；调用方不接触内部备份记录，也不补排后续准备步骤。
pub struct PrepareProfileStartupUseCase {
    backup: Arc<dyn ProfileUpgradeBackupPort>,
    storage: Arc<dyn ProfileStartupStoragePort>,
    lifecycle: Arc<dyn ProfileLifecycleRepositoryPort>,
    target: ProfileUpgradeVersions,
    policy: ProfileUpgradeBackupPolicy,
}

impl PrepareProfileStartupUseCase {
    pub fn new(
        backup: Arc<dyn ProfileUpgradeBackupPort>,
        storage: Arc<dyn ProfileStartupStoragePort>,
        lifecycle: Arc<dyn ProfileLifecycleRepositoryPort>,
        target: ProfileUpgradeVersions,
    ) -> Self {
        Self {
            backup,
            storage,
            lifecycle,
            target,
            policy: ProfileUpgradeBackupPolicy::default(),
        }
    }

    /// 选择备份失败时的启动取舍。见 [`ProfileUpgradeBackupPolicy`]。
    pub fn with_policy(mut self, policy: ProfileUpgradeBackupPolicy) -> Self {
        self.policy = policy;
        self
    }

    pub async fn execute(&self) -> Result<ProfileLifecycle, ProfileStartupError> {
        let backed_up = match self.ensure_backup().await {
            Ok(backed_up) => backed_up,
            Err(error) if self.policy.continues_after_failure() => {
                // 备份不是启动前置条件：降级为告警并按"未备份"继续。用 Debug 打印以
                // 保留 anyhow 的完整错误链（哪一步、哪个 io kind 都在里面）。
                tracing::warn!(
                    target: "profile.upgrade_backup",
                    error = ?error,
                    policy = "warn_and_continue",
                    "profile upgrade backup failed; continuing startup without a pre-upgrade backup",
                );
                false
            }
            Err(error) => return Err(error.into()),
        };
        let existing = self.lifecycle.load()?;
        if let Some(lifecycle) = &existing {
            // 原样现场保留后，清理中的资料交回恢复出厂负责人，不导入或创建新生命周期。
            if lifecycle.state() != ProfileLifecycleState::Ready {
                return Ok(lifecycle.clone());
            }
        }
        if backed_up {
            self.backup
                .preserve_security_materials(&self.target)
                .await?;
        }
        self.storage.adopt_legacy_layout()?;
        self.storage.apply_pending_import()?;
        if let Some(lifecycle) = existing {
            return Ok(lifecycle);
        }
        let lifecycle = ProfileLifecycle::new(ProfileGeneration::new());
        self.lifecycle.compare_and_swap(None, &lifecycle)?;
        Ok(lifecycle)
    }

    async fn ensure_backup(&self) -> Result<bool, ProfileUpgradeBackupError> {
        let state = self.backup.read_source()?;
        let has_known_version = state.source_product.is_some() || state.source_engine.is_some();
        let known_versions_are_current = state
            .source_product
            .as_deref()
            .is_none_or(|version| version == self.target.product)
            && state
                .source_engine
                .as_deref()
                .is_none_or(|version| version == self.target.engine);
        if !state.has_data || (has_known_version && known_versions_are_current) {
            return Ok(false);
        }
        if self.backup.read_prepared_target()?.as_ref() == Some(&self.target) {
            self.backup.verify_prepared(&self.target).await?;
        } else {
            self.backup.capture_verified(&self.target).await?;
        }
        Ok(true)
    }
}
