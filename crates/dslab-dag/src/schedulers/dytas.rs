use crate::dag::DAG;
use crate::data_item::DataTransferMode;
use crate::runner::Config;
use crate::scheduler::{Action, Scheduler};
use crate::schedulers::common::topsort;
use crate::system::System;
use crate::task::*;
use simcore::context::SimulationContext;
use std::collections::VecDeque;

struct QueueSet<T> {
    /// This will contain the processor task queue of each
    /// resource.
    queues: Vec<VecDeque<T>>,
}

impl<T> QueueSet<T> {
    /// Creates a new `QueueSet` with `num_queues` empty queues.
    pub fn new(num_queues: usize) -> Self {
        // Create a Vec with the specified capacity and populate it with new queues.
        let mut queues = Vec::with_capacity(num_queues);
        for _ in 0..num_queues {
            queues.push(VecDeque::new());
        }
        Self { queues }
    }

    /// Expands the number of queues n the QueueSet.
    pub fn add_queues(&mut self, num_queues: usize) {
        for _ in 0..num_queues {
            self.queues.push(VecDeque::new());
        }
    }

    /// Enqueues specified task to the specified processor's task queue.
    /// The queue_index specifies which processor the queue belongs to.
    /// The value is the task_id.
    pub fn enqueue(&mut self, queue_index: usize, value: T) {
        if let Some(queue) = self.queues.get_mut(queue_index) {
            queue.push_back(value);
        } else {
            panic!("Queue index out of bounds");
        }
    }

    /// Dequeues the next task from the specified processor's task queue.
    /// The queue_index specifies which processor the queue belongs to.
    pub fn dequeue(&mut self, queue_index: usize) -> Option<T> {
        self.queues.get_mut(queue_index).and_then(|q| q.pop_front())
    }

    /// This will just return, but not remove, the task id of the specified index.
    /// The queue_index specifies which processor the queue belongs to.
    /// The queue_sub_index is the index within the specified queue to inspect.
    pub fn get_element_mut(&mut self, queue_index: usize, queue_sub_index: usize) -> Option<&mut T> {
        self.queues.get_mut(queue_index)?.get_mut(queue_sub_index)
    }
}

/// Returns a topologically sorted order of task references based on their dependencies.
/// Each task appears after all of its dependencies. If a cycle exists, returns None.
// fn topological_sort<'a>(tasks: &'a [Task]) -> Option<VecDeque<usize>> {
//     let n = tasks.len();
//     // Create a vector to store the in-degree (number of dependencies) of each task.
//     let mut in_degree = vec![0; n];

//     // For each task, the in-degree is the number of tasks it depends on.
//     for (i, task) in tasks.iter().enumerate() {
//         in_degree[i] = task.inputs.len();
//     }

//     // Start with tasks that have no dependencies.
//     let mut queue = VecDeque::new();
//     for i in 0..n {
//         if in_degree[i] == 0 {
//             queue.push_back(i);
//         }
//     }

//     let mut sorted_indices = VecDeque::new();

//     // Process tasks with no remaining dependencies.
//     while let Some(task_id) = queue.pop_front() {
//         sorted_indices.push_back(task_id);

//         // For each task that depends on the current task...
//         for &dependent in &tasks[task_id].outputs {
//             // Decrement the in-degree since one dependency is satisfied.
//             in_degree[dependent] -= 1;
//             // If this task has no more dependencies, add it to the queue.
//             if in_degree[dependent] == 0 {
//                 queue.push_back(dependent);
//             }
//         }
//     }

//     // If we processed all tasks, convert indices to references.
//     if sorted_indices.len() == n {
//         Some(sorted_indices)
//         // let sorted_tasks: VecDeque<&Task> = sorted_indices.into_iter().map(|i| &tasks[i]).collect();
//         // Some(sorted_tasks)
//     } else {
//         // A cycle exists in the dependency graph.
//         None
//     }
// }
pub struct DynamicTaskSchedulingAlgorithm {
    // ptq will be used to keep track of the dag task_id's
    // that are to be executed on each system resource
    // (assuming each "system" is a processor).

    // You can alternatively make this a queue set of
    // strings to store the task's name instead.
    processor_task_queues: QueueSet<usize>,

    // This will contain the task_id's of
    // tasks that have already been completed
    completed_task_queue: Vec<usize>
}

impl DynamicTaskSchedulingAlgorithm {
    pub fn new() -> Self {
        DynamicTaskSchedulingAlgorithm {
            // There are no ptqs when initializing the algorithm
            // but it will expands once it starts.
            processor_task_queues: QueueSet::new(0),
            completed_task_queue: Vec::new()
        }
    }

    pub fn initialize_ptqs(&mut self, system: &System) {
        // self.processor_task_queues = QueueSet::new(system.resources.len());
        self.processor_task_queues.add_queues(system.resources.len());
    }

    pub fn add_task_to_ctq(&mut self, task_id:usize){
        self.completed_task_queue.push(task_id);
    }

    fn schedule(&mut self, dag: &DAG, system: System, ctx: &SimulationContext) -> Vec<Action> {
        for k in 0..system.resources.len() {

        }

        Vec::new()
    }
}

impl Scheduler for DynamicTaskSchedulingAlgorithm {
    fn start(&mut self, dag: &DAG, system: System, config: Config, ctx: &SimulationContext) -> Vec<Action> {
        assert_ne!(
            config.data_transfer_mode,
            DataTransferMode::Manual,
            "DynamicTaskSchedulingAlgorithm doesn't support DataTransferMode::Manual"
        );

        // Initialize processor task queue for each resource,
        // according to the
        // number of resources available.
        self.initialize_ptqs(&system);

        // Getting the tasks from the dag into the
        // initial task queue
        // let initial_task_queue = dag.get_tasks();

        // Sorting the tasks by dependency in the dispatch task queue.

        // NOTE: Consider swapping for topsort in common.rs
        // let mut dispatch_task_queue = topological_sort(&initial_task_queue).expect("Cycle detected in task dependencies");
        let mut dispatch_task_queue = topsort(dag);
        dispatch_task_queue.reverse(); // Reversing so that I can pop easily next in sorted order.

        // Distributing tasks into the processor_task_queues
        while !dispatch_task_queue.is_empty() {
            for i in 0..system.resources.len() {
                
                if dispatch_task_queue.is_empty() {
                    // Just in case the dtq becomes empty during the
                    // inner loop
                    break
                }
                self.processor_task_queues.enqueue(i, dispatch_task_queue.pop().unwrap());
            }
        }

        // The initialization phase is over. Now it is time
        // for the dynamic scheduling part.
        self.schedule(dag, system, ctx)
    }

    fn on_task_state_changed(
        &mut self,
        _task: usize,
        _task_state: TaskState,
        dag: &DAG,
        system: System,
        ctx: &SimulationContext,
    ) -> Vec<Action> {
        self.schedule(dag, system, ctx)
    }

    fn is_static(&self) -> bool {
        false
    }
}
