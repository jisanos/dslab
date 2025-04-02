import pathlib
from wfcommons.wfchef.recipes import *
from wfcommons import WorkflowGenerator


for i in range(103, 10000, 50):
    generator = WorkflowGenerator(SeismologyRecipe.from_num_tasks(i))
    workflow = generator.build_workflow()
    workflow.write_json(pathlib.Path(f"{i}-seismology-workflow.json"))
