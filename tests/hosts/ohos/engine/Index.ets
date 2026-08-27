import engine from 'libuc_ohos_napi.so'

// Re-export the typed NAPI contract from the package entry so HAR consumers
// can import both the runtime default and its public named types.
export * from 'libuc_ohos_napi.so'
export default engine
