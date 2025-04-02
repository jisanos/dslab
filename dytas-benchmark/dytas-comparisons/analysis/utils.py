import os
import argparse
import json

def find_file(target_filename, search_path="."):
    """
    Recursively search for files with the name target_filename starting from search_path.
    
    Parameters:
      search_path (str): The directory to start the search (absolute or relative).
      target_filename (str): The exact name of the file to search for.
      
    Returns:
      Full file path that match the target_filename, or False if
      none was found.
    """

    for root, dirs, files in os.walk(search_path):
        for file in files:
            if file == target_filename:
                return os.path.join(root, file)
    
    print("Error: File not found {}".format(target_filename))
    sys.exit(1)


def calculate_dag_properties(dag):
    """
    Calculate the number of nodes, edges and density of a DAG.
        
    The DAG is assumed to have its tasks stored under:
        dag["workflow"]["tasks"]
        
    Each task is considered a node. The 'parents' field in each task lists its incoming edges.
    """
    # Retrieve tasks (nodes) from the DAG
    tasks = dag["workflow"]["tasks"]
    if not tasks:
        return 0, 0, 0  # If no tasks, then nodes, edges and density are all zero.

    num_nodes = len(tasks)

    # For graphs with fewer than 2 nodes, density is not defined (we return 0)
    if num_nodes < 2:
        return num_nodes, 0, 0.0
    
    # Sum up the edges from the length of the "parents" list for each task.
    num_edges = sum(len(task["parents"]) for task in tasks)
    
    # Compute density using the DAG density equation
    # (Actual number of edges vs maximum number of possible edges)
    density = (2 * num_edges) / (num_nodes * (num_nodes - 1))

    return num_nodes, num_edges, density


def calculate_dag_properties_helper(dag_path):
    with open(dag_path, "r") as f:
        dag = json.load(f)
        
    return calculate_dag_properties(dag)
    