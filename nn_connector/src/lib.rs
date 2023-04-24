#![warn(clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![deny(clippy::cargo_common_metadata)]
#![deny(
    clippy::all,
    clippy::pedantic,
)]
#![allow(
    clippy::unreadable_literal,
    clippy::type_repetition_in_bounds,
    clippy::missing_errors_doc,
    clippy::cast_possible_truncation,
    clippy::used_underscore_binding,
    clippy::ptr_as_ptr,
    clippy::missing_panics_doc,
    clippy::missing_docs_in_private_items,
    clippy::module_name_repetitions,
    clippy::unreadable_literal
)]

use pyo3::{exceptions::{PyRuntimeError, PyTimeoutError, PyStopIteration}, prelude::*};
#[allow(unused)]
use pyo3::create_exception;

use std::collections::HashMap;

pub mod passive;
pub mod active;
pub mod error;

use passive::FuzzConnector;
use active::FuzzConnector as ActiveFuzzConnector;

#[pyclass]
#[repr(transparent)]
struct PyFuzzConnector(FuzzConnector);

#[pymethods]
impl PyFuzzConnector {
    #[new]
    pub fn new(port: u16) -> PyResult<Self> {
        
        let conn = match FuzzConnector::new(port) {
            Ok(conn) => conn,
            Err(e) => {
                return Err(PyErr::new::<PyRuntimeError, _>(e.to_string()));
            }
        };

        Ok(Self(conn))
    }

    pub fn send_input(&mut self, input: &[u8]) -> PyResult<bool> {
        match self.0.send_input(input) {
            Ok(_) => Ok(true),
            Err(e) => Err(PyErr::new::<PyRuntimeError, _>(e.to_string())),
        }
    }

    pub fn recv_input(&mut self) -> PyResult<HashMap<String, Vec<u8>>> {
        match self.0.recv_testcase() {
            Ok(map) => Ok(map),
            Err(error::Error::NotAvailable()) => Err(PyErr::new::<PyTimeoutError, _>("read timeout expired")),
            Err(error::Error::SerializeError(msg)) => Err(PyErr::new::<PyTimeoutError, _>(msg)),
            Err(e) => Err(PyErr::new::<PyRuntimeError, _>(e.to_string())),
        }
    }

    pub fn id(&self) -> u32 {
        self.0.id()
    }
}

#[pyclass]
#[repr(transparent)]
struct PyFuzzActiveConnector(ActiveFuzzConnector);

#[pymethods]
impl PyFuzzActiveConnector {
    #[new]
    pub fn new(model_name: &str, port: u16) -> PyResult<Self> {
        let conn = match ActiveFuzzConnector::new(model_name.to_string(), port) {
            Ok(conn) => conn,
            Err(e) =>  {
                return Err(PyErr::new::<PyRuntimeError, _>(e.to_string()));
            }
        };

        Ok(Self(conn))
    }

    pub fn recv_input(&mut self) -> PyResult<HashMap<String, Vec<u8>>> {
        match self.0.recv_input() {
            Ok(res) => Ok(res),
            Err(error::Error::NotAvailable()) => Err(PyErr::new::<PyTimeoutError, _>("read timeout expired")),
            Err(error::Error::SerializeError(msg)) => Err(PyErr::new::<PyTimeoutError, _>(msg)),
            Err(e) => Err(PyErr::new::<PyRuntimeError, _>(e.to_string())),
        }
    }
    

    pub fn send_heatmap(&mut self, heatmap: Vec<u32>) -> PyResult<bool> {
        match self.0.send_heatmap(heatmap) {
            Ok(_) => Ok(true),
            Err(e) => Err(PyErr::new::<PyRuntimeError, _>(e.to_string())),
        }
    }

    pub fn recv_map(&mut self) -> PyResult<HashMap<String, Vec<u8>>> {
        match self.0.recv_map() {
            Ok(res) => Ok(res),
            Err(error::Error::StopIteration()) => Err(PyErr::new::<PyStopIteration, _>("end of mutation stage")),
            Err(error::Error::NotAvailable()) => Err(PyErr::new::<PyTimeoutError, _>("read timeout expired")),
            Err(error::Error::SerializeError(msg)) => Err(PyErr::new::<PyTimeoutError, _>(msg)),
            Err(e) => Err(PyErr::new::<PyRuntimeError, _>(e.to_string())),
        }
    }

}


/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn nn_connector(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyFuzzConnector>()?;
    m.add_class::<PyFuzzActiveConnector>()?;
    Ok(())
}
