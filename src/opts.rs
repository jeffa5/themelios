use clap::Parser;

#[derive(Parser, Debug)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: SubCmd,

    #[clap(long, short, global = true, default_value = "1")]
    pub apps_per_client: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub clients: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub schedulers: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub datastores: usize,

    #[clap(long, short, global = true, default_value = "1")]
    pub nodes: usize,

    #[clap(long, default_value = "8080")]
    pub port: u16,
}

#[derive(clap::Subcommand, Debug)]
pub enum SubCmd {
    Serve,
    CheckDfs,
    CheckBfs,
    CheckSimulation { seed: Option<u64> },
}
