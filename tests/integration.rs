#[cfg(test)]
mod tests {
    use anyhow::Error;
    use std::time::Instant;
    use wasi_cap_std_sync::WasiCtxBuilder;
    use wasi_experimental_http_wasmtime::HttpCtx;
    use wasmtime::*;
    use wasmtime_wasi::Wasi;

    // We run the same test in a Tokio and non-Tokio environment
    // in order to make sure both scenarios are working.

    #[test]
    #[should_panic]
    fn test_none_allowed() {
        setup_tests(None, None);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[should_panic]
    async fn test_async_none_allowed() {
        setup_tests(None, None);
    }

    #[test]
    #[should_panic]
    fn test_without_allowed_domains() {
        setup_tests(Some(vec![]), None);
    }

    #[tokio::test(flavor = "multi_thread")]
    #[should_panic]
    async fn test_async_without_allowed_domains() {
        setup_tests(Some(vec![]), None);
    }

    #[test]
    fn test_with_allowed_domains() {
        setup_tests(
            Some(vec![
                "https://api.brigade.sh".to_string(),
                "https://postman-echo.com".to_string(),
            ]),
            None,
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_async_with_allowed_domains() {
        setup_tests(
            Some(vec![
                "https://api.brigade.sh".to_string(),
                "https://postman-echo.com".to_string(),
            ]),
            None,
        );
    }

    #[test]
    #[should_panic]
    fn test_concurrent_requests_rust() {
        let module = "target/wasm32-wasi/release/simple_wasi_http_tests.wasm".to_string();
        make_concurrent_requests(module);
    }
    #[tokio::test(flavor = "multi_thread")]
    #[should_panic]
    async fn test_async_concurrent_requests_rust() {
        let module = "target/wasm32-wasi/release/simple_wasi_http_tests.wasm".to_string();
        make_concurrent_requests(module);
    }

    #[test]
    #[should_panic]
    fn test_concurrent_requests_as() {
        let module = "tests/as/build/optimized.wasm".to_string();
        make_concurrent_requests(module);
    }

    fn make_concurrent_requests(module: String) {
        let func = "concurrent";
        let instance = create_instance(
            module,
            Some(vec!["https://api.brigade.sh".to_string()]),
            Some(2),
        )
        .unwrap();
        let func = instance
            .get_func(func)
            .unwrap_or_else(|| panic!("cannot find function {}", func));

        func.call(&[]).unwrap();
    }

    fn setup_tests(allowed_domains: Option<Vec<String>>, max_concurrent_requests: Option<u32>) {
        let modules = vec![
            "target/wasm32-wasi/release/simple_wasi_http_tests.wasm",
            "tests/as/build/optimized.wasm",
        ];
        let test_funcs = vec!["get", "post"];

        for module in modules {
            let instance = create_instance(
                module.to_string(),
                allowed_domains.clone(),
                max_concurrent_requests,
            )
            .unwrap();
            run_tests(&instance, &test_funcs).unwrap();
        }
    }

    /// Execute the module's `_start` function.
    fn run_tests(instance: &Instance, test_funcs: &[&str]) -> Result<(), Error> {
        for func_name in test_funcs.iter() {
            let func = instance
                .get_func(func_name)
                .unwrap_or_else(|| panic!("cannot find function {}", func_name));
            func.call(&[])?;
        }

        Ok(())
    }

    /// Create a Wasmtime::Instance from a compiled module and
    /// link the WASI imports.
    fn create_instance(
        filename: String,
        allowed_domains: Option<Vec<String>>,
        max_concurrent_requests: Option<u32>,
    ) -> Result<Instance, Error> {
        let start = Instant::now();
        let store = Store::default();
        let mut linker = Linker::new(&store);

        let ctx = WasiCtxBuilder::new()
            .inherit_stdin()
            .inherit_stdout()
            .inherit_stderr()
            .build()?;

        let wasi = Wasi::new(&store, ctx);
        wasi.add_to_linker(&mut linker)?;
        // Link `wasi_experimental_http`
        let http = HttpCtx::new(allowed_domains, max_concurrent_requests)?;
        http.add_to_linker(&mut linker)?;

        let module = wasmtime::Module::from_file(store.engine(), filename)?;

        let instance = linker.instantiate(&module)?;
        let duration = start.elapsed();
        println!("module instantiation time: {:#?}", duration);
        Ok(instance)
    }
}
