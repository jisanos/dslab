import json
import matplotlib.pyplot as plt
import re
import argparse
from collections import defaultdict


def plot_exec_time_vs_graph_size(json_file_path):
    """
    Reads a JSON file containing benchmark results and plots execution time vs. graph size
    for different workflow types.

    Parameters:
        json_file_path (str): Path to the JSON file.
    """
    # Load the JSON data
    with open(json_file_path, "r") as file:
        data = json.load(file)

    # Dictionary to hold data for each workflow type
    workflow_data = defaultdict(list)

    # Extract the number of nodes and execution times for each workflow type
    for entry in data:
        match = re.match(r"(\d+)-(.+)", entry["dag"])
        if match:
            num_nodes = int(match.group(1))
            workflow_name = match.group(2)
            exec_time = entry["exec_time"]
            workflow_data[workflow_name].append((num_nodes, exec_time))

    # Plotting
    plt.figure(figsize=(12, 6))

    for workflow_name, values in workflow_data.items():
        # Sort values by number of nodes
        values.sort()
        num_nodes, exec_times = zip(*values)
        plt.plot(num_nodes, exec_times, marker='o',
                 linestyle='-', label=workflow_name)

    plt.xlabel("Graph Size (Number of Nodes)")
    plt.ylabel("Execution Time (seconds)")
    plt.title("Execution Time vs Graph Size for Different Workflows")
    plt.legend(title="Workflow Type")
    plt.grid(True)
    plt.show()


# Example usage
plot_exec_time_vs_graph_size("./benchmark-results.json")
