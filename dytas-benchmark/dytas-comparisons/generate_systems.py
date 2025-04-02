#!/usr/bin/env python3
import os
import sys
import itertools
import yaml
import copy


def update_filename(input_filename, new_model, new_bandwidth, new_latency):
    """
    Update the filename by replacing the last three dash-separated parts with
    new values (keeping the original extension).
    Example:
      Input: cluster-het-4-32-ConstantBW-1250-100.yaml
      Output: cluster-het-4-32-<new_model>-<new_bandwidth>-<new_latency>.yaml
    """
    base, ext = os.path.splitext(input_filename)
    parts = base.split('-')
    if len(parts) < 3:
        raise ValueError(
            "Filename does not contain at least three '-' separated parts.")
    # Remove last three parts (old model, bandwidth, latency)
    new_base = "-".join(parts[:-3])
    new_filename = f"{new_base}-{new_model}-{new_bandwidth}-{new_latency}{ext}"
    return new_filename


def main():
    if len(sys.argv) < 2:
        print("Usage: python generate_permutations.py <input_yaml_file>")
        sys.exit(1)

    input_file = sys.argv[1]

    # Read the input YAML file
    with open(input_file, 'r') as f:
        try:
            data = yaml.safe_load(f)
        except yaml.YAMLError as exc:
            print("Error parsing YAML file:", exc)
            sys.exit(1)

    # Lists of values to permute
    models = ["SharedBandwidth", "ConstantBandwidth"]
    bandwidths = [125, 1250, 12500]
    latencies = [10, 100, 1000]

    # Create an output directory to avoid cluttering the current directory
    output_dir = "generated_files"
    os.makedirs(output_dir, exist_ok=True)

    count = 0
    # Iterate over all combinations
    for model_val, bw_val, lat_val in itertools.product(models, bandwidths, latencies):
        # Make a deep copy to ensure nested dictionaries are not affected by subsequent changes
        new_data = copy.deepcopy(data)
        # Update only the desired nested network properties
        new_data['network']['model'] = model_val
        new_data['network']['bandwidth'] = bw_val
        new_data['network']['latency'] = lat_val

        # Generate new file name based on the input filename
        new_filename = update_filename(os.path.basename(
            input_file), model_val, bw_val, lat_val)
        new_filepath = os.path.join(output_dir, new_filename)

        # Write the new YAML file
        with open(new_filepath, 'w') as out_file:
            yaml.dump(new_data, out_file, default_flow_style=False)
        print(f"Generated: {new_filepath}")
        count += 1

    print(f"\nTotal files generated: {count}")


if __name__ == "__main__":
    main()
