use crate::dag::DAG;
use crate::data_item::DataTransferMode;
use crate::runner::Config;
use crate::scheduler::SchedulerParams;
use crate::scheduler::{Action, Scheduler};
use crate::schedulers::common::task_successors;
use crate::schedulers::common::topsort;
use crate::system::System;
use crate::task::TaskState;
use simcore::context::SimulationContext;
use std::collections::VecDeque;
use std::str::FromStr;
use strum_macros::Display;
use strum_macros::EnumIter;
use strum_macros::EnumString;

struct Resource {
    id: usize,
    cores: u32,
    cores_available: u32,
    memory: u64,
    memory_available: u64,
    speed: f64
}

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
    // pub fn dequeue(&mut self, queue_index: usize) -> Option<T> {
    //     self.queues.get_mut(queue_index).and_then(|q| q.pop_front())
    // }

    /// This will just return, but not remove, the task id of the specified index.
    /// The queue_index specifies which processor the queue belongs to.
    /// The queue_sub_index is the index within the specified queue to inspect.
    pub fn get_element_mut(&mut self, queue_index: usize, queue_sub_index: usize) -> Option<&mut T> {
        self.queues.get_mut(queue_index)?.get_mut(queue_sub_index)
    }

    /// Returns and removes element from queue in specified subindex.
    pub fn remove_element(&mut self, queue_index: usize, queue_sub_index: usize) -> Option<T> {
        self.queues.get_mut(queue_index)?.remove(queue_sub_index)
    }

    // pub fn get_longest_queue_length(&mut self) -> usize {
    //     let mut max_length = 0;
    //     for i in 0..self.queues.len() {
    //         if self.queues[i].len() > max_length {
    //             max_length = self.queues[i].len();
    //         }
    //     }
    //     max_length
    // }

    pub fn queue_len(&mut self, queue_index: usize) -> usize {
        self.queues[queue_index].len()
    }

    // pub fn get_queue(&mut self, index: usize) -> Option<&mut VecDeque<T>> {
    //     self.queues.get_mut(index)
    // }
}

/// Validates that the task is able to be executed in the specified resource.
pub fn evaluate(dag: &DAG, task_id: usize, resources: &Vec<Resource>, resource: usize) -> bool {
    let need_cores = dag.get_task(task_id).min_cores;
    if resources[resource].cores_available < need_cores {
        return false;
    }
    let need_memory = dag.get_task(task_id).memory;
    if resources[resource].memory_available < need_memory {
        return false;
    }
    if !dag.get_task(task_id).is_allowed_on(resource) {
        return false;
    }
    true
}

#[derive(Clone, Debug, PartialEq, Display, EnumIter, EnumString)]
pub enum PTQNavigationCriterion {
    Whole, // Validate all tasks of current processor's task queue (PTQ_k[0...n]) before verifying the next processor's task queue (PTQ_{k+1}[0...n])
    Front, // Only validates the task in the front of the current processor's task queue (PTQ_k[0]) before verifying the next processor's task queue (PTQ_{k+1}[0])
}
#[derive(Clone, Debug, PartialEq, Display, EnumIter, EnumString)]
pub enum TaskSortingCriterion {
    Khan, // Khan's topological sorting algorithm
    DFS,  // Depth First Search topological sort implemented in dslab
}
#[derive(Clone, Debug, PartialEq, Display, EnumIter, EnumString)]
pub enum MultiCoreCriterion {
    SkipActiveNodes, // If a processor is active, skip it. Only one task is assigned at a time per processor.
    UseAllCores,     // Fill up all of a processor's cores even if the processor is active to maximize core usage.
}

#[derive(Clone, Debug)]
pub struct Strategy {
    pub ptq_navigation_criterion: PTQNavigationCriterion,
    pub task_sorting_criterion: TaskSortingCriterion,
    pub multi_core_criterion: MultiCoreCriterion,
}

impl Strategy {
    pub fn from_params(params: &SchedulerParams) -> Self {
        let ptq_navigation_criterion_str: String = params.get("navigation").unwrap();
        let task_sorting_criterion_str: String = params.get("sorting").unwrap();
        let multi_core_criterion_str: String = params.get("multicore").unwrap();
        Self {
            ptq_navigation_criterion: PTQNavigationCriterion::from_str(&ptq_navigation_criterion_str)
                .expect("Wrong criterion: {ptq_navigation_criterion_str}"),
            task_sorting_criterion: TaskSortingCriterion::from_str(&task_sorting_criterion_str)
                .expect("Wrong criterion: {task_sorting_criterion_str}"),
            multi_core_criterion: MultiCoreCriterion::from_str(&multi_core_criterion_str)
                .expect("Wrong criterion: {multi_core_criterion_str}"),
        }
    }
}

/// Based on Kahn’s Algorithm on topological sorting.
pub fn khan_topological_sort(dag: &DAG) -> Vec<usize> {
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
    pub strategy: Strategy,
}

impl DynamicTaskSchedulingAlgorithm {
    pub fn new(strategy: Strategy) -> Self {
        DynamicTaskSchedulingAlgorithm {
            // There are no ptqs when initializing the algorithm
            // but it will expands once it starts.
            processor_task_queues: QueueSet::new(0),
            strategy,
        }
    }

    pub fn from_params(params: &SchedulerParams) -> Self {
        Self::new(Strategy::from_params(params))
    }

    fn initialize_ptqs(&mut self, system: &System) {
        // self.processor_task_queues = QueueSet::new(system.resources.len());
        self.processor_task_queues.add_queues(system.resources.len());
    }

    fn schedule(&mut self, dag: &DAG, system: System, _ctx: &SimulationContext) -> Vec<Action> {
        let mut result = Vec::new();

        // Cloning the current state of resources
        let mut resources: Vec<Resource> = system
            .resources
            .iter()
            .enumerate()
            .map(|(id, resource)| Resource {
                id,
                cores: resource.cores,
                cores_available: resource.cores_available,
                memory: resource.memory,
                memory_available: resource.memory_available,
                speed: resource.speed,
            })
            .collect();

        let resources_length = resources.len();

        for k in 0..resources_length {
            // Validate if the k'th processors is in
            // a running state. This will be determined
            // by validateing the number of available cores.

            if self.strategy.multi_core_criterion == MultiCoreCriterion::SkipActiveNodes
                && resources[k].cores_available != resources[k].cores
            {
                // This will be taken to indicate that
                // the resource is in running state,
                // thus we'll skip to the next processor
                continue;
            } else if self.strategy.multi_core_criterion == MultiCoreCriterion::UseAllCores
                && resources[k].cores_available == 0
            {
                // skip if no cores are available.
                continue;
            }

            // This loop accesses the ptq_k, but if no tasks in it are
            // ready for execution, the loop navigates through PTQ_{k+1} and so on
            // to get a suitable task.
            'outer: for i in 0..resources_length {
                // Queue index should go from k..n and then from 0..k
                let queue_index = (i + k) % resources_length;

                let n_queue_sub_index_navigation =
                    if self.strategy.ptq_navigation_criterion == PTQNavigationCriterion::Whole {
                        self.processor_task_queues.queue_len(queue_index) // navigates all elements in PTQ_{queue_index}
                    } else if self.strategy.ptq_navigation_criterion == PTQNavigationCriterion::Front {
                        1 // Only use front element in PTQ_{queue_index} before checking the next PTQ
                    } else {
                        1
                    };
                for queue_sub_index in 0..n_queue_sub_index_navigation {
                    let task_id = self.processor_task_queues.get_element_mut(queue_index, queue_sub_index);

                    // 1st: Validate that ptq is not empty.
                    // 2nd: Validate that all of the required
                    // task ids have been completed before executing.
                    // 3rd: Make sure the task is executable
                    // in the specified resource.
                    if task_id != None
                        && dag.get_task(**task_id.as_ref().unwrap()).state == TaskState::Ready
                        && evaluate(dag, **task_id.as_ref().unwrap(), &resources, k)
                    {
                        let task = &dag.get_task(**task_id.as_ref().unwrap());

                        // NOTE: Cores management isn't discussed for DYTAS... So i implemented
                        // a very basic assignment of cores here.
                        let cores_to_assign = if task.max_cores > resources[k].cores {
                            resources[k].cores
                        } else {
                            task.max_cores
                        };

                        // Schedule the task into the k'th processor,
                        // if it passes the requirements to be
                        // assigned.
                        result.push(Action::ScheduleTask {
                            task: self
                                .processor_task_queues
                                .remove_element(queue_index, queue_sub_index)
                                .unwrap(),
                            resource: k,
                            cores: cores_to_assign,
                            expected_span: None,
                        });

                        if self.strategy.multi_core_criterion == MultiCoreCriterion::UseAllCores {
                            resources[k].cores_available -= cores_to_assign;
                            resources[k].memory_available -= task.memory;
                        } else {
                            break 'outer; // Breaking loop to go to the next processor
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

        // Initialize processor task queue for each resource (processor),
        // according to the number of resources available.
        self.initialize_ptqs(&system);

        // Sorting the tasks by dependency in the dispatch task queue.
        // NOTE: Verify if changing the sorting methodology affects performance.
        // let mut dispatch_task_queue = topsort(dag);
        // dispatch_task_queue.reverse(); // Reversing so that I can pop easily next in sorted order.

        // NOTE: This sorting methodology achieves better makespan results than
        // the one above.
        // let mut dispatch_task_queue = khan_topological_sort(dag);
        // dispatch_task_queue.reverse();

        let mut dispatch_task_queue = if self.strategy.task_sorting_criterion == TaskSortingCriterion::DFS {
            topsort(dag)
        } else if self.strategy.task_sorting_criterion == TaskSortingCriterion::Khan {
            khan_topological_sort(dag)
        } else {
            khan_topological_sort(dag)
        };

        // Cloning the current state of resources
        let mut resources: Vec<Resource> = system
            .resources
            .iter()
            .enumerate()
            .map(|(id, resource)| Resource {
                id,
                cores: resource.cores,
                cores_available: resource.cores_available,
                memory: resource.memory,
                memory_available: resource.memory_available,
                speed: resource.speed,
            })
            .collect();

        // dispatch_task_queue = vec![25, 0, 18, 7, 6, 3, 2, 1, 13, 5, 9, 15, 4, 16, 11, 8, 14, 19, 12, 10, 22, 20, 17, 23, 21, 24, 26];
        // dispatch_task_queue.reverse();

        dispatch_task_queue.reverse();
        // Distributing tasks into the processor_task_queues in round robin fashion.
        'outer: loop {
            for i in 0..system.resources.len() {
                let Some(task_id) = dispatch_task_queue.last() else {
                    // If dispatch task queue becomes empty: break out.
                    break 'outer;
                };

                // Verifying if the task is allowed on the processor
                // before enqueuing it into its ptq
                if evaluate(dag, *task_id, &resources, i) {
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
