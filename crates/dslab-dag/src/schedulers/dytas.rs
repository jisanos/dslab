use crate::dag::DAG;
use crate::data_item::DataTransferMode;
use crate::runner::Config;
use crate::scheduler::{Action, Scheduler};
use crate::schedulers::common::task_successors;
use crate::schedulers::common::topsort;
use crate::system::System;
use crate::task::TaskState;
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

    // pub fn get_queue(&mut self, index: usize) -> Option<&mut VecDeque<T>> {
    //     self.queues.get_mut(index)
    // }
}

/// Validates that the task is able to be executed in the specified resource.
pub fn evaluate(dag: &DAG, task_id: usize, system: &System, resource: usize) -> bool {
    let need_cores = dag.get_task(task_id).min_cores;
    if system.resources[resource].compute.borrow().cores_total() < need_cores {
        return false;
    }
    let need_memory = dag.get_task(task_id).memory;
    if system.resources[resource].compute.borrow().memory_total() < need_memory {
        return false;
    }
    if !dag.get_task(task_id).is_allowed_on(resource) {
        return false;
    }
    true
}

/// Based on Kahn’s Algorithm on topological sorting.
pub fn topological_sort(dag: &DAG) -> Vec<usize> {
    let mut in_degree = vec![0; dag.get_tasks().len()];
    let mut sorted_tasks = Vec::new();
    let mut queue = VecDeque::new();

    // Compute in-degree (number of dependencies) for each task
    for task_id in 0..dag.get_tasks().len() {
        let predecessors = task_predecessors(task_id, dag);
        in_degree[task_id] = predecessors.len();
    }

    // Find tasks with no dependencies (in-degree = 0)
    for (task_id, &degree) in in_degree.iter().enumerate() {
        if degree == 0 {
            queue.push_back(task_id);
        }
    }

    // Process tasks in topological order
    while let Some(task_id) = queue.pop_front() {
        sorted_tasks.push(task_id);

        for (successor_id, _) in task_successors(task_id, dag) {
            in_degree[successor_id] -= 1;

            // If a task has no remaining dependencies, add it to the queue
            if in_degree[successor_id] == 0 {
                queue.push_back(successor_id);
            }
        }
    }

    // Ensure the DAG has no cycles
    if sorted_tasks.len() != dag.get_tasks().len() {
        panic!("Cycle detected in the DAG!");
    }

    sorted_tasks
}

pub fn task_predecessors(v: usize, dag: &DAG) -> Vec<(usize, f64)> {
    let mut result = Vec::new();
    for &data_item_id in dag.get_task(v).inputs.iter() {
        let data_item = dag.get_data_item(data_item_id);
        result.push((data_item.producer.unwrap(), data_item.size));
    }
    result
}

pub struct DynamicTaskSchedulingAlgorithm {
    // ptq will be used to keep track of the dag task_id's
    // that are to be executed on each system resource
    // (assuming each "system" is a processor).
    processor_task_queues: QueueSet<usize>,
}

impl DynamicTaskSchedulingAlgorithm {
    pub fn new() -> Self {
        DynamicTaskSchedulingAlgorithm {
            // There are no ptqs when initializing the algorithm
            // but it will expands once it starts.
            processor_task_queues: QueueSet::new(0),
        }
    }

    fn initialize_ptqs(&mut self, system: &System) {
        // self.processor_task_queues = QueueSet::new(system.resources.len());
        self.processor_task_queues.add_queues(system.resources.len());
    }

    fn schedule(&mut self, dag: &DAG, system: System, _ctx: &SimulationContext) -> Vec<Action> {
        let mut result = Vec::new();

        for (k, resource) in system.resources.iter().enumerate() {
            // Validate if the k'th processors is in
            // a running state. This will be determined
            // by validateing the number of available cores.

            if resource.cores_available != resource.cores {
                // This will be taken to indicate that
                // the resource is in running state,
                // thus we'll skip to the next processor
                continue;
            }

            for i in 0..system.resources.len() {
                // Queue index should go from k..n and then from 0..k
                let queue_index = (i + k) % system.resources.len();

                // First loop accesses the ptq_k, but if the task is not
                // ready for execution, the loop navigates through ptq_k+1 and so on
                // to get a suitable task.
                let task_id = self.processor_task_queues.get_element_mut(queue_index, 0);

                // 1st: Validate that ptq is not empty.
                // 2nd: Validate that all of the required
                // task ids have been completed before executing.
                // 3rd: Make sure the task is executable
                // in the specified resource.
                if task_id != None
                    && dag.get_task(**task_id.as_ref().unwrap()).state == TaskState::Ready
                    && evaluate(dag, **task_id.as_ref().unwrap(), &system, k)
                {
                    let task = &dag.get_task(**task_id.as_ref().unwrap());
                    let cores_to_assign = if task.max_cores > resource.cores {
                        resource.cores
                    } else {
                        task.max_cores
                    };

                    // Schedule the task into the k'th processor,
                    // if it passes the requirements to be
                    // assigned.
                    result.push(Action::ScheduleTask {
                        task: self.processor_task_queues.dequeue(queue_index).unwrap(),
                        resource: k,
                        cores: cores_to_assign,
                        expected_span: None,
                    });
                    break; // Breaking loop to go to the next processor
                }
            }
        }
        result
    }
}

impl Scheduler for DynamicTaskSchedulingAlgorithm {
    fn start(&mut self, dag: &DAG, system: System, config: Config, ctx: &SimulationContext) -> Vec<Action> {
        assert_ne!(
            config.data_transfer_mode,
            DataTransferMode::Manual,
            "DynamicTaskSchedulingAlgorithm doesn't support DataTransferMode::Manual"
        );

        // Initialize processor task queue for each resource (processor),
        // according to the number of resources available.
        self.initialize_ptqs(&system);

        // Sorting the tasks by dependency in the dispatch task queue.
        // NOTE: Verify if changing the sorting methodology affects performance.
        // let mut dispatch_task_queue = topsort(dag);
        // dispatch_task_queue.reverse(); // Reversing so that I can pop easily next in sorted order.

        // NOTE: This sorting methodology achieves better makespan results than
        // the one above.
        let mut dispatch_task_queue = topological_sort(dag);
        dispatch_task_queue.reverse();

        // dispatch_task_queue = vec![25, 0, 18, 7, 6, 3, 2, 1, 13, 5, 9, 15, 4, 16, 11, 8, 14, 19, 12, 10, 22, 20, 17, 23, 21, 24, 26];
        // dispatch_task_queue.reverse();
        // Distributing tasks into the processor_task_queues
        'outer: loop {
            for i in 0..system.resources.len() {
                let Some(task_id) = dispatch_task_queue.last() else {
                    // If dispatch task queue becomes empty: break out.
                    break 'outer;
                };

                // Verifying if the task is allowed on the processor
                // before enqueuing it into its ptq
                if evaluate(dag, *task_id, &system, i) {
                    self.processor_task_queues
                        .enqueue(i, dispatch_task_queue.pop().unwrap()); // Pop to confirm removing from dtq
                }
            }
        }

        // The initialization phase is over.
        // Now it is time for the dynamic scheduling part.
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
