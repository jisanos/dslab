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

    pub fn get_queue(&mut self, index: usize) -> Option<&mut VecDeque<T>> {
        self.queues.get_mut(index)
    }
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
    // This will contain the task_id's of
    // tasks that have already been completed

    // Given that i can validate if all
    // task dependencies are met using
    // the tasks state, I woulnd't need
    // a list of completed tasks here.
    // This can be done with task.inputs.len() == task.ready_inputs.
    // This can also be done veryfing if task is in Ready state, as
    // if it is still Pending it means that its dependencies
    // have not been resolved yet.
    // completed_task_queue: Vec<usize>,
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

            if resource.cores_available < resource.cores {
                // This will be taken to indicate that
                // the resource is in running state,
                // thus we'll skip to the next processor
                continue;
            }

            let ptq_k: &mut VecDeque<usize> = self.processor_task_queues.get_queue(k).unwrap();

            // Page 26 says that:
            // "If any PTQ completes its Task set at the earliest, migrate the suitable task from the available PTQ‟s."
            // This means I might have to include a migration of tasks
            // here from tasks in other's PTQs that have yet to be scheduled.
            // thus, likely not a continue, but the else statement below.

            // Now, if the next task in ptq_k has
            // its dependent tasks resolved, then we
            // execute it.
            // let task_id = ptq_k.front().unwrap().clone();

            // 1st: Validate that ptq is not empty.
            // 2nd: Validate that all of the required
            // task ids have been completed before executing.
            // 3rd: Make sure the task is executable
            // in the specified resource.
            if ptq_k.front() != None
                && dag.get_task(ptq_k.front().unwrap().clone()).state == TaskState::Ready
                && evaluate(dag, ptq_k.front().unwrap().clone(), &system, k)
            {
                let task = &dag.get_task(ptq_k.front().unwrap().clone());
                // Schedule the task into the k'th processor,
                // if it passes the requirements to be
                // assigned.
                result.push(Action::ScheduleTask {
                    task: ptq_k.pop_front().unwrap(),
                    resource: k,
                    cores: task.max_cores, // TODO: Might have to determine cores based on task's min-max cores and resource cores.
                    expected_span: None,
                });
            } else {
                // Otherwise, we navigate to the other
                // ptqs and verify if they contain a task that
                // meet the criteria.

                // NOTE: I'm only validating the front task in each queue,
                // but I might have to change it so that it takes
                // into consideration their other queued tasks as well.
                for z in 0..system.resources.len() {
                    if z == k {
                        // NOTE: I dont think this if should be here since
                        // it should also verify other tasks down the line
                        // within the same processor task queue during this process.
                        // based on what the pseudocode states.
                        continue; // Skip current ptq
                    }

                    let ptq_z = self.processor_task_queues.get_queue(z).unwrap();

                    if ptq_z.is_empty() {
                        // no need to proceed if no tasks are in
                        // this queue
                        continue;
                    }
                    let task_id = ptq_z.front().unwrap().clone();

                    let task = &dag.get_task(task_id);

                    if task.state == TaskState::Ready && evaluate(dag, task_id, &system, k) {
                        // Schedule task on k'th processor
                        result.push(Action::ScheduleTask {
                            task: ptq_z.pop_front().unwrap(),
                            resource: k,
                            cores: task.max_cores,
                            expected_span: None,
                        });
                        break; // Removing this break alone increases the makespan. Please leave it. It also alings with the paper's implementation.
                    }
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
