use anyhow::{bail, Error};
use structopt::StructOpt;
use wasi_cap_std_sync::WasiCtxBuilder;
use wasi_experimental_http_wasmtime::HttpCtx;
use wasmtime::{Func, Instance, Linker, Store, Val, ValType};
use wasmtime_wasi::Wasi;

#[derive(Debug, StructOpt)]
#[structopt(name = "wasmtime-http")]
struct Opt {
    #[structopt(help = "The path of the WebAssembly module to run")]
    module: String,

    #[structopt(
        short = "i",
        long = "invoke",
        default_value = "_start",
        help = "The name of the function to run"
    )]
    invoke: String,

    #[structopt(
        short = "e",
        long = "env",
        value_name = "NAME=VAL",
        parse(try_from_str = parse_env_var),
        help = "Pass an environment variable to the program"
    )]
    vars: Vec<(String, String)>,

    #[structopt(
        short = "a",
        long = "allowed-host",
        help = "Host the guest module is allowed to make outbound HTTP requests to"
    )]
    allowed_hosts: Option<Vec<String>>,

    #[structopt(
        short = "c",
        long = "concurrency",
        help = "The maximum number of concurrent requests a module can make to allowed hosts"
    )]
    max_concurrency: Option<u32>,

    #[structopt(value_name = "ARGS", help = "The arguments to pass to the module")]
    module_args: Vec<String>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Error> {
    let opt = Opt::from_args();
    let method = opt.invoke.clone();
    // println!("{:?}", opt);
    let instance = create_instance(opt.module, opt.vars, opt.allowed_hosts, opt.max_concurrency)?;
    let func = instance
        .get_func(method.as_str())
        .unwrap_or_else(|| panic!("cannot find function {}", method));

    invoke_func(func, opt.module_args)?;

    Ok(())
}

fn create_instance(
    filename: String,
    vars: Vec<(String, String)>,
    allowed_hosts: Option<Vec<String>>,
    max_concurrent_requests: Option<u32>,
) -> Result<Instance, Error> {
    let store = Store::default();
    let mut linker = Linker::new(&store);

    let ctx = WasiCtxBuilder::new()
        .inherit_stdin()
        .inherit_stdout()
        .inherit_stderr()
        .envs(&vars)?
        .build()?;

    let wasi = Wasi::new(&store, ctx);
    wasi.add_to_linker(&mut linker)?;
    // Link `wasi_experimental_http`
    let http = HttpCtx::new(allowed_hosts, max_concurrent_requests)?;
    http.add_to_linker(&mut linker)?;

    let module = wasmtime::Module::from_file(store.engine(), filename)?;
    let instance = linker.instantiate(&module)?;

    Ok(instance)
}

// Invoke function given module arguments and print results.
// Adapted from https://github.com/bytecodealliance/wasmtime/blob/main/src/commands/run.rs.
fn invoke_func(func: Func, args: Vec<String>) -> Result<(), Error> {
    let ty = func.ty();

    let mut args = args.iter();
    let mut values = Vec::new();
    for ty in ty.params() {
        let val = match args.next() {
            Some(s) => s,
            None => {
                bail!("not enough arguments for invocation")
            }
        };
        values.push(match ty {
            ValType::I32 => Val::I32(val.parse()?),
            ValType::I64 => Val::I64(val.parse()?),
            ValType::F32 => Val::F32(val.parse()?),
            ValType::F64 => Val::F64(val.parse()?),
            t => bail!("unsupported argument type {:?}", t),
        });
    }

    let results = func.call(&values)?;
    for result in results.into_vec() {
        match result {
            Val::I32(i) => println!("{}", i),
            Val::I64(i) => println!("{}", i),
            Val::F32(f) => println!("{}", f32::from_bits(f)),
            Val::F64(f) => println!("{}", f64::from_bits(f)),
            Val::ExternRef(_) => println!("<externref>"),
            Val::FuncRef(_) => println!("<funcref>"),
            Val::V128(i) => println!("{}", i),
        };
    }

    Ok(())
}

fn parse_env_var(s: &str) -> Result<(String, String), Error> {
    let parts: Vec<_> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        bail!("must be of the form `key=value`");
    }
    Ok((parts[0].to_owned(), parts[1].to_owned()))
}
