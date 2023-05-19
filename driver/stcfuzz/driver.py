import os
import subprocess
import psutil
import toml
from datetime import datetime
import time


class FuzzSession:
    def __init__(self, config_path: str, start_core: int = 0, debug=False) -> None:
        self.debug = debug
        self.session = {}
        self.cores_busy = start_core
        self.fuzz_path = os.path.abspath("./fuzz_results")
        with open(config_path, "r") as file:
            config = file.read()
            self.config = toml.loads(config)
        self.fuzz_opts_func = {
            "spawn_client": (lambda x: "-S " if x is True else None),
            "spawn_broker": (lambda x: None if x is True else "-B "),
            "type": (lambda x: f"{os.path.abspath(self.binaries[x])} "),
            "cores": (lambda x: f"-c {self.get_core_nums(x)} "),
        }

        self.fuzz_opts = {
            "broker_port": "--broker-port {0} ",
            "client_port": "--client-port {0} ",
            "nn_slave_port": "--port {0} ",
            "seed": "--seed {0} ",
            "timeout": "timeout -s SIGINT {0} ",
            "execution_timeout": "-t {0} ",
        }
        self.fuzz_path_opts = {
            "fuzz_path": "{0}",
            "bin_path": "{0} ",
            "harness_path": "{0} ",
            "stdout": "--stdout {0} ",
            "input_path": "-i {0} ",
            "dict_path": "-x {0} ",
            "solutions_path": "-o {0} ",
            "log_path": "{0}",
            "queue_path": "-q {0} ",
        }
        self.binaries = {
            "fuzz": "./nn_fuzz",
            "slave": "./nn_slave",
        }

    def create(self):
        global_conf = self.process_conf(self.config["global"], "global")
        os.makedirs(self.fuzz_path, exist_ok=True)

        for name, conf in self.config["proc"].items():
            config = {**global_conf, **self.process_conf(conf, name)}

            if self.debug:
                print(f'Parsed config for "{name}": {config}')
            self.session[name] = FuzzProcess(
                process_str=self.conf_to_str(**config),
                debug=self.debug,
                cwd=config["fuzz_path"],
                log_path=config["log_path"],
            )

        if "client" in self.config.keys():
            self.session["client"] = FuzzProcess(
                process_str=self.config["client"]["start_str"],
                debug=self.debug,
                cwd=self.fuzz_path,
                log_path=f"{self.fuzz_path}/client.log",
            )
        if self.debug:
            print("START CMDS:")
            for k, v in self.start_cmd().items():
                print(f"{k}: {v}")

    def process_conf(self, conf, name):
        conf_with_flags = {}
        if name != "global" and name != "client":
            initial_config = {**self.initial_conf(name), **conf}
        else:
            initial_config = conf
        for k, v in initial_config.items():
            if k == "fuzz_path" and name == "global":
                self.fuzz_path = os.path.abspath(v)
            elif k == "type":
                conf_with_flags["bin_path"] = self.fuzz_opts_func[k](v)
            elif k in self.fuzz_opts_func:
                flag = self.fuzz_opts_func[k](v)
                if flag:
                    conf_with_flags[k] = flag
            elif k in self.fuzz_opts:
                conf_with_flags[k] = self.fuzz_opts[k].format(v)
            elif k in self.fuzz_path_opts:
                conf_with_flags[k] = self.fuzz_path_opts[k].format(os.path.abspath(v))
            else:
                raise Exception(f'THERE IS NO "{k}" OPTION!')
        return conf_with_flags

    def initial_conf(self, name):
        fuzz_path = f"{self.fuzz_path}/{name}/"
        os.makedirs(fuzz_path, exist_ok=True)
        return {
            "fuzz_path": fuzz_path,
            "stdout": f"{fuzz_path}fuzz.out",
            "log_path": f"{fuzz_path}stats.log",
            "solutions_path": f"{fuzz_path}solutions/",
            "queue_path": f"{fuzz_path}corpus_discovered/",
            "cores": "1",
        }

    def get_core_nums(self, n):
        n = int(n)
        if self.cores_busy + n > psutil.cpu_count():
            raise Exception(f"ALL {psutil.cpu_count()} CORES ARE BUSY!")
        elif n <= 0:
            raise Exception("NUMBER OF CORES MUST BE MORE THEN ZERO")
        if n == 1:
            cores = f"{self.cores_busy}"
            self.cores_busy += 1
        elif n > 1:
            cores = f"{self.cores_busy}-{self.cores_busy + n - 1}"
            self.cores_busy += n
        return cores

    def run(self):
        for name, fuzzer in self.session.items():
            print(f'Starting "{name}" process!')
            fuzzer.run()

    def status(self):
        processes_status = {}
        for name, fuzzer in self.session.items():
            processes_status.update({name: fuzzer.status()})
        return processes_status

    def start_cmd(self):
        start_cmds = {}
        for name, fuzzer in self.session.items():
            start_cmds.update({name: str(fuzzer)})
        start_cmds.update({"cwd": self.fuzz_path})
        return start_cmds

    def terminate(self):
        for name, fuzzer in self.session.items():
            fuzzer.kill()
            print(f'Process "{name}" is killed')

    @staticmethod
    def conf_to_str(
        bin_path: str,
        harness_path: str,
        timeout: str = "",
        input_path: str = "",
        dict_path: str = "",
        cores: str = "",
        spawn_broker: str = "",
        broker_port: str = "",
        spawn_client: str = "",
        client_port: str = "",
        stdout: str = "",
        solutions_path: str = "",
        seed: str = "",
        execution_timeout: str = "",
        queue_path: str = "",
        **kwargs,
    ):
        return "".join(
            [
                timeout,
                bin_path,
                spawn_client,
                client_port,
                spawn_broker,
                broker_port,
                queue_path,
                input_path,
                dict_path,
                execution_timeout,
                seed,
                cores,
                stdout,
                solutions_path,
                f"{harness_path}-- @@",
            ]
        )


class FuzzProcess:
    def __init__(
        self,
        process_str: str,
        cwd: str = "./",
        log_path: str = "process_out.log",
        debug: bool = False,
    ) -> None:
        self.cwd = cwd
        self.log_path = log_path
        self.debug = debug
        self.process = None
        self.process_str = process_str

    def run(self):
        if self.process is None:
            with open(f"{self.log_path}", "wb") as out:
                self.process = subprocess.Popen(
                    self.process_str,
                    shell=True,
                    stdout=out,
                    stderr=subprocess.STDOUT,
                    cwd=self.cwd,
                )
            print("Succesfully started!")
            if self.debug:
                print(
                    f"PID: {self.process.pid} CWD: {self.cwd} LOG_PATH: {self.log_path}"
                )
                print("=-=-=-=-=-=-=-=-=-=-=-=")
        else:
            print(f"PROCESS {self.process.pid} ALREADY EXISTS")

    def pid(self):
        if self.process is None:
            raise Exception("THERE IS NO RUNNING FUZZ PROCESS!")
        return self.process.pid

    def status(self):
        if self.process is None:
            return "created"
        else:
            return (
                "working"
                if self.process.poll() is None
                else f"stopped: {self.process.poll()}"
            )

    def kill(self):
        if self.process is None:
            raise Exception("THERE IS NO RUNNING FUZZ PROCESS!")
        else:
            try:
                parent = psutil.Process(self.process.pid)
                for child in parent.children(recursive=True):
                    if self.debug:
                        print(f"CHILD: {child}")
                        print(f"PARENT: {parent}")
                    child.kill()
                    parent.kill()
            except Exception:
                print("PROCESS DOES NOT EXIST")

    def __str__(self):
        return self.process_str


def main():
    args = get_args_parser().parse_args()
    if len(args.config) == 0:
        raise Exception('Provide config file via "--config"')
    session = FuzzSession(args.config, args.start_core, args.debug)
    session.create()
    session.run()
    while True:
        print(f"{datetime.now().time()}")
        print(session.status())
        print("=-=-=-=-=-=-=-=-=-=-=-=")
        time.sleep(args.print_every)


def get_args_parser(add_help=True):
    import argparse

    parser = argparse.ArgumentParser(
        description="Python driver for Rust fuzzer", add_help=add_help
    )

    parser.add_argument(
        "--config",
        dest="config",
        default="",
        type=str,
        action="store",
        help="path to fuzzing session config",
    )

    parser.add_argument(
        "--print-every",
        dest="print_every",
        default=60 * 5,
        type=int,
        action="store",
        help="print processes info every N seconds",
    )

    parser.add_argument(
        "--start-core",
        dest="start_core",
        default=0,
        type=int,
        action="store",
        help="The number of the core from which the other cores are counted",
    )

    parser.add_argument(
        "--debug",
        dest="debug",
        default=False,
        type=bool,
        action="store",
        help="debug mode",
    )
    return parser


if __name__ == "__main__":
    main()
