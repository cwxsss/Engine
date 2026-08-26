use std::fs;
use std::path::{Path, PathBuf};

use uc_ohos_napi::core_version;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(relative: &str) -> String {
    let path = workspace_root().join(relative);
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

#[test]
fn core_version_uses_the_binding_package_version() {
    assert_eq!(core_version(), format!("v{}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn member_sync_preferences_are_publicly_exposed_by_ohos_binding() {
    let library = read("bindings/uc-ohos-napi/src/lib.rs");
    let runtime = read("bindings/uc-ohos-napi/src/runtime.rs");
    let declarations = read("bindings/uc-ohos-napi/ohos/index.d.ts");

    for symbol in [
        "pub struct OhContentTypes",
        "pub struct OhContentTypesPatch",
        "pub struct OhMemberSyncPreferences",
        "pub struct OhMemberSyncPreferencesPatch",
    ] {
        assert!(library.contains(symbol), "missing N-API object: {symbol}");
    }
    for method in [
        "pub async fn query_member_sync_preferences",
        "pub async fn update_member_sync_preferences",
        "Operation::QueryMemberSyncPreferences",
        "Operation::UpdateMemberSyncPreferences",
    ] {
        assert!(
            runtime.contains(method),
            "missing runtime contract: {method}"
        );
    }
    for declaration in [
        "export interface OhContentTypes",
        "export interface OhContentTypesPatch",
        "export interface OhMemberSyncPreferences",
        "export interface OhMemberSyncPreferencesPatch",
        "queryMemberSyncPreferences(deviceId: string): Promise<OhMemberSyncPreferences>",
        "updateMemberSyncPreferences(deviceId: string, patch: OhMemberSyncPreferencesPatch): Promise<OhMemberSyncPreferences>",
    ] {
        assert!(
            declarations.contains(declaration),
            "missing ArkTS declaration: {declaration}"
        );
    }
}
