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

    pub fn get_queue(&mut self, index: usize) -> Option<&mut VecDeque<T>> {
        self.queues.get_mut(index)
    }
}

pub struct DynamicTaskSchedulingAlgorithm {
    // ptq will be used to keep track of the dag task_id's
    // that are to be executed on each system resource
    // (assuming each "system" is a processor).

    // You can alternatively make this a queue set of
    // strings to store the task's name instead.
    processor_task_queues: QueueSet<usize>,

    // This will contain the task_id's of
    // tasks that have already been completed

    // NOTE: Instead of this, I can
    // just go to the dag and check all
    // tasks that are in the state 'Done'.
    completed_task_queue: Vec<usize>,
}

impl DynamicTaskSchedulingAlgorithm {
    pub fn new() -> Self {
        DynamicTaskSchedulingAlgorithm {
            // There are no ptqs when initializing the algorithm
            // but it will expands once it starts.
            processor_task_queues: QueueSet::new(0),
            completed_task_queue: Vec::new(),
        }
    }

    fn initialize_ptqs(&mut self, system: &System) {
        // self.processor_task_queues = QueueSet::new(system.resources.len());
        self.processor_task_queues.add_queues(system.resources.len());
    }

    fn add_task_to_ctq(&mut self, task_id: usize) {
        self.completed_task_queue.push(task_id);
    }
    // TODO: Fix these errors:
    // [23.200 ERROR runner] Wrong action, resource 3 doesn't have enough cores
    // [23.200 ERROR runner] DAG is not completed, currently 1 Pending, 1 Ready, 4 Done tasks
    fn schedule(&mut self, dag: &DAG, system: System, ctx: &SimulationContext) -> Vec<Action> {
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

            if ptq_k.is_empty(){
                // no need to proceed if no tasks
                // are in this queue


                // NOTE: This might have to be changed later to
                // switch to a different ptq so that the cpu isnt
                // left running idly.
                continue;
            }
            // Now, if the next task in ptq_k has
            // its dependent tasks resolved, then we
            // execute it.

            let task_id = ptq_k.front().unwrap().clone();
            let task = &dag.get_task(task_id);
            // let mut required_data_items_ids = task.inputs;
            
            // println!("{:?}", required_data_items_ids);

            if task.inputs.is_empty() {
                // If there are no input tasks
                // then this is a starting task
                // and it can be scheduled as is

                if dag.get_task(task_id).is_allowed_on(k) {
                    result.push(Action::ScheduleTask {
                        task: ptq_k.pop_front().unwrap(),
                        resource: k,
                        cores: task.max_cores,
                        expected_span: None,
                    });

                    self.completed_task_queue.push(task_id);
                }
            } else {
                // Otherwise, validate that all of the required
                // task ids have been completed before executing.

                // let mut requirements_met = required_tasks_ids
                //     .iter()
                //     .all(|&id| self.completed_task_queue.contains(&id));



                if task.inputs.len() == task.ready_inputs {
                    // Schedule the task into the k'th processor.

                    if dag.get_task(task_id).is_allowed_on(k) {
                        result.push(Action::ScheduleTask {
                            task: ptq_k.pop_front().unwrap(),
                            resource: k,
                            cores: task.max_cores,
                            expected_span: None,
                        });

                        self.completed_task_queue.push(task_id);
                    }
                } else {
                    // Otherwise, we navigate to the other
                    // ptqs and verify if they contain a task that
                    // meet the criteria.

                    // NOTE: I'm only validating the front task in each queue,
                    // but I might have to change it so that it takes
                    // into consideration their other queued tasks as well.
                    for z in 0..system.resources.len() {
                        if z == k {
                            continue; // Skip current ptq
                        }

                        let ptq_z = self.processor_task_queues.get_queue(z).unwrap();

                        if ptq_z.is_empty(){
                            // no need to proceed if no tasks are in
                            // this queue
                            continue;
                        }
                        let task_id = ptq_z.front().unwrap().clone();


                        let task = &dag.get_task(task_id);

                        // required_tasks_ids = &dag.get_task(task_id).inputs;

                        if task.inputs.is_empty() {
                            // No requirements means that it is good to be
                            // executed

                            if dag.get_task(task_id).is_allowed_on(k) {
                                result.push(Action::ScheduleTask {
                                    task: ptq_z.pop_front().unwrap(),
                                    resource: k,
                                    cores: task.max_cores,
                                    expected_span: None,
                                });

                                self.completed_task_queue.push(task_id);
                            }
                        } else {
                            // requirements_met = required_tasks_ids
                            //     .iter()
                            //     .all(|&id| self.completed_task_queue.contains(&id));

                            if task.inputs.len() == task.ready_inputs {
                                // Schedule task on k'th processor

                                if dag.get_task(task_id).is_allowed_on(k) {
                                    result.push(Action::ScheduleTask {
                                        task: ptq_z.pop_front().unwrap(),
                                        resource: k,
                                        cores: task.max_cores,
                                        expected_span: None,
                                    });

                                    self.completed_task_queue.push(task_id);
                                }
                            } else {
                                // Otherwise, check next ptq
                                continue;
                            }
                        }
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
                    break;
                }
                let task_id = dispatch_task_queue.last().unwrap();
                // Verifying if the task is allowed on the processor
                // before enqueuing it into its ptq
                if dag.get_task(*task_id).is_allowed_on(i) {
                    self.processor_task_queues
                        .enqueue(i, dispatch_task_queue.pop().unwrap());
                }
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
