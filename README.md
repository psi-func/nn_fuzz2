# NN fuzz toolchain

Small toolchain for fuzzing with nn power

## Tools

- LibAFL-based multiprocess fuzzer with afl-forkserver backend and neural network connection support
- Python communication client

## Getting started

1. Install the Dependencies

    - LLVM tools
    The LLVM tools (including clang, clang++) are needed (newer than LLVM 11.0.0 but older than LLVM 15.0.0)

    - Python (>= 3.7) (only for client)

    - LibAFL

    **NOTE:** nn_fuzz ^0.2.0 depends on LibAFL 0.9

    Clone from [link](https://github.com/AFLplusplus/LibAFL)

2. Build projects:

     - Fuzzer

     ```sh
    cargo build -p nn_fuzz --release
    ```

     - Python client

     ```sh
       pip install maturin
       cd nn_connector
       maturin build --release
       pip install target/wheels/nn_connector*
     ```
