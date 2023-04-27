# Driver for fuzzing

Сonfiguration must be described as a toml file, with folowing structure:

```
[global]
#global options

[proc.FIRST_INSTANCE_NAME]
# options for first instance

[proc.SECOND_INSTANCE_NAME]
# options for second instance

# more "proc." instances

[client]
start_str = "Comand to run client"
```

## Options

| Option | Description |
| :-- | :-- |
|Spawn_client | If set True, spawn message passing with remote receivers.|
|Spawn_broker | If set true, spawn broker for fuzzers [default: true]|
|Broker_port | The port broker listen to accept new instances. [default: 1337]|
|Client_port | The port to which nn-client will be bind. [default: 7878]|
|Cores | Spawn a client in each of the provided cores. Use 'all' to select all available cores. 'none' to run a client without binding to any core. ex: '1,2-4,6' selects the cores 1, 2, 3, 4, and 6. |
|Seed |The list of seeds for random generator per core, current_nanos if "auto" Must be not less than cores list len! Example: 703,12,0-10 |
|Timeout | Process running time. After timeout, the process will be killed with SIGINT. |
|Fuzz_path | Path where the fuzzing session will be executed. |
|Type | Type of binary to use. {"fuzz": "./nn_fuzz", "slave": "./nn_slave"} |
| Bin_path | Path to excutable binary. Used instead of "Type" option [default: ./nn_fuzz and ./nn_slave] |
|Harness_path | Path to harness (program under test). |
|Stdout | The file to write output from fuzzer instances. |
|Input_path | The directory to read initial corpus, generate inputs if undefined.|
|Dict_path | Path to token file for token mutations.|
|Solutions_path | Path to directory where solutions are stored.|
|Log_path | Path to file where process logs are.| 
|Queue_path | Path directory where corpus is stored |

Сan be executed both in the terminal and by hand as a module:

* Terminal:

```bash
python fuzz_driver.py --config config.toml --print-every 300 --debug True
```

* As a module:

```python
from fuzz_driver import FuzzSession
session = FuzzSession("./fuzz_conf.toml", debug = True)
session.create() # Parsing config. Making all dirs.
for k, v in session.start_cmd().items(): # Comands to be executed.
    print(f"{k}: {v}")
for k, v in session.status().items(): # Status of processes.
    print(f"{k}: {v}")
a.run() # Run all the processes.
for k, v in session.status().items(): # Status of processes.
    print(f"{k}: {v}")
#session.terminate() # Kill all working processes.
```

