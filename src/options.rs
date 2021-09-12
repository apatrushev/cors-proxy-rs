use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
pub(crate) struct Opt {
    /// Listen port
    #[structopt(short, long, default_value = "3000")]
    pub(crate) port: u16,
}
