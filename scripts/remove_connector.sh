#!/usr/bin/env bash

pip uninstall nn_connector -y \
&& maturin build --release \
&& pip install target/wheels/nn_connector*
