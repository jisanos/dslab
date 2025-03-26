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
// use dslab_dag::scheduler::default_scheduler_resolver;
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
    let tasks_json: Vec<_> = dag.get_tasks().iter().enumerate().map(|(id, task)|{
        json!({
            "id":id,
            "name":task.name,
            "flops":task.flops,
            "memory": task.memory,
            "min_cores": task.min_cores,
            "max_cores": task.max_cores,
            "parent": task_predecessors(id, &dag).iter().map(|(first, _)| *first).collect::<Vec<usize>>(),
            "children": task_successors(id, &dag).iter().map(|(first, _)| *first).collect::<Vec<usize>>()
        })
    }).collect();

    println!("{}", serde_json::to_string_pretty(&tasks_json).unwrap());
}
