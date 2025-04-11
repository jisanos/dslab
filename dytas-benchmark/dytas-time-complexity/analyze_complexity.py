import json
import numpy as np
import pandas as pd
import re
import argparse
from scipy.optimize import curve_fit


# Define power-law function: O(t^p)
def power_law(t, p, c):
    return c * np.power(t, p)


# Define exponential function: O(e^(b*t))
def exponential_fit(t, a, b):
    return a * np.exp(b * t)


# Function to fit complexity models
def fit_complexity(data, workflow_name):
    # Extract number of nodes and execution times for the given workflow
    graph_sizes = []
    exec_times = []

    for entry in data:
        if workflow_name in entry["dag"]:
            match = re.match(r"(\d+)-", entry["dag"])
            if match:
                graph_sizes.append(int(match.group(1)))
                exec_times.append(entry["exec_time"])

    # Convert to numpy arrays
    graph_sizes = np.array(graph_sizes, dtype=float)
    exec_times = np.array(exec_times, dtype=float)

    # Ensure enough data points for fitting
    if len(graph_sizes) < 3:
        return {
            "workflow": workflow_name,
            "best_fit": "Insufficient data",
            "power_law_O": "N/A",
            "exponential_O": "N/A"
        }

    # Fit power-law model: O(t^p)
    power_params, _ = curve_fit(
        power_law, graph_sizes, exec_times, p0=[1, 1], maxfev=5000)
    estimated_p, estimated_c = power_params
    power_predictions = power_law(graph_sizes, estimated_p, estimated_c)
    power_mse = np.mean((exec_times - power_predictions) ** 2)  # Compute MSE

    # Fit exponential model: O(e^(b*t))
    exp_params, _ = curve_fit(exponential_fit, graph_sizes, exec_times, p0=[
                              1, 0.00001], maxfev=5000)
    estimated_a, estimated_b = exp_params
    exp_predictions = exponential_fit(graph_sizes, estimated_a, estimated_b)
    exp_mse = np.mean((exec_times - exp_predictions) ** 2)  # Compute MSE

    # Determine the best fit based on lower MSE
    best_fit = "Power Law" if power_mse < exp_mse else "Exponential"

    return {
        "workflow": workflow_name,
        "best_fit": best_fit,
        "power_law_O": f"O(t^{estimated_p:.2f})",
        "exponential_O": f"O(e^({estimated_b:.6f} * t))"
    }


def main():
    # Command-line argument parsing
    parser = argparse.ArgumentParser(
        description="Analyze benchmark results and determine Big-O complexity.")
    parser.add_argument("json_file", type=str,
                        help="Path to the benchmark-results.json file")
    args = parser.parse_args()

    # Load JSON data
    with open(args.json_file, "r") as file:
        data = json.load(file)

    # Workflows to analyze
    workflows = ["epigenomics", "seismology", "montage",
                 "blast", "bwa", "cycles", "genome", "seismology", "soykb", "srasearch"]
    complexity_results = [fit_complexity(
        data, workflow) for workflow in workflows]

    # Convert results to a Pandas DataFrame
    df_complexity = pd.DataFrame(complexity_results)

    # Print results
    print(df_complexity.to_string(index=False))


if __name__ == "__main__":
    main()
