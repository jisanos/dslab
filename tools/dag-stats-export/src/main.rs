// use std::fs::File;
// use std::io::{BufWriter, Write};
use std::path::PathBuf;
// use serde::ser::{Serialize, Serializer, SerializeStruct};
use clap::Parser;

use dslab_dag::dag::DAG;
use dslab_dag::schedulers::dytas::task_predecessors;
use dslab_dag::schedulers::common::task_successors;

use dslab_dag::parsers::config::ParserConfig;
use serde_json::json;
// use dslab_dag::scheduler::default_scheduler_resdolver;
// use serde::Serialize;

#[derive(Parser, Debug)]
#[command(about, long_about = None)]
/// Runs batch experiment with DSLab DAG
struct Args {
    /// Path to DAG file
    #[arg(short, long)]
    dag_path: PathBuf,
}

fn main() {
    env_logger::init();
    let args = Args::parse();
    let dag = DAG::from_file(&args.dag_path, &ParserConfig::default());
    let dag_stats = dag.stats();

    let dag_stats_json = serde_json::to_string_pretty(&dag_stats).unwrap();


    println!("{}", dag_stats_json);
}
