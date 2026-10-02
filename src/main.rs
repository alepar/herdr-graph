fn main() -> std::process::ExitCode {
    // Crash-injection points for subprocess tests (`HG_FAILPOINTS`), only in `test-support` builds.
    #[cfg(feature = "test-support")]
    herdr_graph::failpoint::arm_from_env();
    herdr_graph::cli::main()
}
