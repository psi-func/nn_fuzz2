import os
import subprocess
import psutil
import toml
from datetime import datetime
import time


class FuzzSession:
    def __init__(self, config_path: str, debug=False) -> None:
        self.debug = debug
        self.session = {}
        self.fuzz_path = os.path.abspath("./fuzz_results")
        with open(config_path, "r") as file:
            config = file.read()
            self.config = toml.loads(config)
        self.fuzz_flags = {
            "spawn_client": (lambda x: "-S " if x is True else None),
            "spawn_broker": (lambda x: None if x is True else "-B "),
            "type": (lambda x: os.path.abspath(self.binaries[x])),
        }

        self.fuzz_opts = {
            "cores": "-c {0} ",
            "broker_port": "--broker-port {0} ",
            "client_port": "--client-port {0} ",
            "seed": "--seed {0} ",
            "timeout": "timeout -s SIGINT {0} ",
        }
        self.fuzz_path_opts = {
            "fuzz_path": "{0} ",
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
            "fuzz": "./nn_fuzz ", 
            "slave": "./nn_slave ",
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
                print(f'Start string for CLIENT: {self.config["client"]["start_str"]}')

    def run(self):
        for name, fuzzer in self.session.items():
            fuzzer.run()
            print(f'Starting "{name}" process!')

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

    def process_conf(self, conf, name):
        if name != "global" and name != "client":
            conf_with_flags = self.get_paths(name)
        else:
            conf_with_flags = {}
        for k, v in conf.items():
            if k == "fuzz_path" and name == "global":
                self.fuzz_path = os.path.abspath(v)
            elif k == "type":
                conf_with_flags["bin_path"] = self.fuzz_flags[k](v)
            elif k in self.fuzz_flags:
                flag = self.fuzz_flags[k](v)
                if flag:
                    conf_with_flags[k] = flag
            elif k in self.fuzz_opts:
                conf_with_flags[k] = self.fuzz_opts[k].format(v)
            elif k in self.fuzz_path_opts:
                conf_with_flags[k] = self.fuzz_path_opts[k].format(os.path.abspath(v))
            else:
                raise Exception(f'THERE IS NO "{k}" OPTION!')
        return conf_with_flags

    def get_paths(self, name):
        os.makedirs(f"{self.fuzz_path}/{name}/", exist_ok=True)
        return {
            "fuzz_path": f"{self.fuzz_path}/{name}/",
            "stdout": self.fuzz_path_opts["stdout"].format("fuzz.out"),
            "log_path": f"{self.fuzz_path}/{name}/stats.log",
            "solutions_path": self.fuzz_path_opts["solutions_path"].format(
                "./solutions/"
            ),
            "queue_path": self.fuzz_path_opts["queue_path"].format(
                "./corpus_discovered/"
            ),
        }

    @staticmethod
    def conf_to_str(
        bin_path: str,
        harness_path: str,
        timeout: str = "",
        input_path: str = "",
        dict_path: str = "",
        cores: str = "",
        broker_port: str = "",
        spawn_client: str = "",
        client_port: str = "",
        stdout: str = "",
        solutions_path: str = "",
        seed: str = "",
        queue_path: str = "",
        **kwargs,
    ):
        return "".join(
            [
                timeout,
                bin_path,
                spawn_client,
                client_port,
                broker_port,
                queue_path,
                input_path,
                dict_path,
                seed,
                cores,
                stdout,
                solutions_path,
                f"{harness_path}@@",
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


def main(args):
    if len(args.config) == 0:
        raise Exception('Provide config file via "--config"')
    session = FuzzSession(args.config, args.debug)
    session.create()
    session.run()
    while True:
        print(f"{datetime.now().time()}")
        print(session.status())
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
        default="60*5",
        type=int,
        action="store",
        help="print processes info every N seconds",
    )

    parser.add_argument(
        "--debug",
        dest="debug",
        default="False",
        type=bool,
        action="store",
        help="debug mode",
    )
    return parser


if __name__ == "__main__":
    args = get_args_parser().parse_args()
    main(args)
