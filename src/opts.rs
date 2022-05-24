use clap::Parser;

#[derive(Parser, Debug)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: SubCmd,

    #[clap(long, short, global = true, default_value = "2")]
    pub schedulers: usize,

    #[clap(long, short, global = true, default_value = "2")]
    pub api_servers: usize,

    #[clap(long, default_value = "8080")]
    pub port: u16,
}

#[derive(clap::Subcommand, Debug)]
pub enum SubCmd {
    Serve,
    CheckDfs,
    CheckBfs,
}
