// Portions derived from outlines-core and modified by OC-Sidememory contributors.
// See the repository PROVENANCE.md and MODIFICATIONS.md.

//! Provides tools and interfaces to integrate the crate's functionality with Python.

use std::collections::VecDeque;
use std::sync::Arc;

use bincode::{config, Decode, Encode};
use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict, PyMemoryView};
use pyo3::wrap_pyfunction;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
#[cfg(feature = "huggingface-hub")]
use tokenizers::FromPretrainedParameters;

use oc_sidememory::index::Index;
use oc_sidememory::json_schema;
use oc_sidememory::prelude::*;
use oc_sidememory::sidememory::{
    ArenaError, CounterError, GuideError as CoreGuideError, HistoryError, ImportError,
    JournalError, ProbeDecision, RegisterError, SemanticViolation as CoreSemanticViolation,
    SequenceDecision,
};

create_exception!(oc_sidememory, OcSidememoryError, PyException);
create_exception!(oc_sidememory, CompileError, OcSidememoryError);
create_exception!(oc_sidememory, StructuralRejection, OcSidememoryError);
create_exception!(oc_sidememory, SemanticViolation, OcSidememoryError);
create_exception!(oc_sidememory, ResourceLimitError, OcSidememoryError);
create_exception!(oc_sidememory, LifecycleError, OcSidememoryError);
create_exception!(oc_sidememory, RollbackError, OcSidememoryError);
create_exception!(oc_sidememory, InternalInvariantError, OcSidememoryError);

const SERIALIZATION_MAGIC: &[u8; 3] = b"OCS";
const SERIALIZATION_VERSION: u8 = 1;

fn core_error(error: oc_sidememory::Error) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn guide_error(error: CoreGuideError) -> PyErr {
    let message = error.to_string();
    match error {
        CoreGuideError::StructuralRejection { .. } | CoreGuideError::UnknownToken { .. } => {
            StructuralRejection::new_err(message)
        }
        CoreGuideError::SemanticRejection(violation) => semantic_error(violation),
        CoreGuideError::Resource(_)
        | CoreGuideError::Arena(ArenaError::Resource(_))
        | CoreGuideError::History(HistoryError::Resource(_))
        | CoreGuideError::History(HistoryError::Arena(ArenaError::Resource(_)))
        | CoreGuideError::History(HistoryError::Journal(JournalError::Resource(_)))
        | CoreGuideError::Counter(CounterError::Resource(_))
        | CoreGuideError::Counter(CounterError::Journal(JournalError::Resource(_)))
        | CoreGuideError::Register(RegisterError::Resource(_))
        | CoreGuideError::Register(RegisterError::Journal(JournalError::Resource(_)))
        | CoreGuideError::Import(ImportError::Resource(_))
        | CoreGuideError::Import(ImportError::Arena(ArenaError::Resource(_)))
        | CoreGuideError::Import(ImportError::History(HistoryError::Resource(_)))
        | CoreGuideError::Journal(JournalError::Resource(_)) => {
            ResourceLimitError::new_err(message)
        }
        CoreGuideError::InvalidLifecycle { .. } => LifecycleError::new_err(message),
        CoreGuideError::RollbackUnavailable { .. } | CoreGuideError::Rollback(_) => {
            RollbackError::new_err(message)
        }
        CoreGuideError::InternalInvariant { .. } | CoreGuideError::Poisoned { .. } => {
            InternalInvariantError::new_err(message)
        }
        _ => OcSidememoryError::new_err(message),
    }
}

fn semantic_error(violation: CoreSemanticViolation) -> PyErr {
    let (message, category, frame) = match &violation {
        CoreSemanticViolation::DuplicateArrayItem { frame, .. } => {
            ("duplicate array item", "duplicate_array_item", *frame)
        }
        CoreSemanticViolation::ContainsMinimumNotMet { frame, .. } => (
            "contains minimum not met",
            "contains_minimum_not_met",
            *frame,
        ),
        CoreSemanticViolation::ContainsMaximumExceeded { frame, .. } => (
            "contains maximum exceeded",
            "contains_maximum_exceeded",
            *frame,
        ),
        CoreSemanticViolation::ContainsMinimumUnreachable { frame, .. } => (
            "contains minimum is unreachable",
            "contains_minimum_unreachable",
            *frame,
        ),
        CoreSemanticViolation::MissingCapture { object, .. } => {
            ("capture is not available", "missing_capture", *object)
        }
        CoreSemanticViolation::EqualityMismatch { object, .. } => (
            "captured value does not match",
            "equality_mismatch",
            *object,
        ),
        CoreSemanticViolation::InequalityMismatch { object, .. } => (
            "captured value must be different",
            "inequality_mismatch",
            *object,
        ),
        CoreSemanticViolation::ImportedMemberRequired { object, .. } => (
            "value is not in the imported set",
            "imported_member_required",
            *object,
        ),
        CoreSemanticViolation::ImportedMemberForbidden { object, .. } => (
            "value is in the forbidden imported set",
            "imported_member_forbidden",
            *object,
        ),
    };
    let error = SemanticViolation::new_err(message);
    Python::attach(|py| {
        let value = error.value(py);
        let _ = value.setattr("category", category);
        let _ = value.setattr("frame_slot", frame.slot);
        let _ = value.setattr("frame_generation", frame.generation);
        match &violation {
            CoreSemanticViolation::DuplicateArrayItem {
                constraint,
                item_index,
                ..
            } => {
                let _ = value.setattr("constraint", constraint.get());
                let _ = value.setattr("item_index", item_index);
            }
            CoreSemanticViolation::ContainsMinimumNotMet {
                constraint,
                processed,
                matched,
                lower,
                ..
            } => {
                let _ = value.setattr("constraint", constraint.get());
                let _ = value.setattr("processed", processed);
                let _ = value.setattr("matched", matched);
                let _ = value.setattr("lower", lower);
            }
            CoreSemanticViolation::ContainsMaximumExceeded {
                constraint,
                item_index,
                processed,
                matched,
                upper,
                ..
            } => {
                let _ = value.setattr("constraint", constraint.get());
                let _ = value.setattr("item_index", item_index);
                let _ = value.setattr("processed", processed);
                let _ = value.setattr("matched", matched);
                let _ = value.setattr("upper", upper);
            }
            CoreSemanticViolation::ContainsMinimumUnreachable {
                constraint,
                item_index,
                processed,
                matched,
                lower,
                ..
            } => {
                let _ = value.setattr("constraint", constraint.get());
                let _ = value.setattr("item_index", item_index);
                let _ = value.setattr("processed", processed);
                let _ = value.setattr("matched", matched);
                let _ = value.setattr("lower", lower);
            }
            CoreSemanticViolation::MissingCapture {
                property, capture, ..
            }
            | CoreSemanticViolation::EqualityMismatch {
                property, capture, ..
            }
            | CoreSemanticViolation::InequalityMismatch {
                property, capture, ..
            } => {
                let _ = value.setattr("property", property.as_ref());
                let _ = value.setattr("capture", capture.as_ref());
            }
            CoreSemanticViolation::ImportedMemberRequired {
                property,
                import_set,
                ..
            }
            | CoreSemanticViolation::ImportedMemberForbidden {
                property,
                import_set,
                ..
            } => {
                let _ = value.setattr("property", property.as_ref());
                let _ = value.setattr("import_set", import_set.as_ref());
            }
        }
    });
    error
}

#[pyclass(name = "CompiledSchema", module = "oc_sidememory._native", frozen)]
pub struct PyCompiledSchema {
    inner: Arc<CompiledSchema>,
}

#[pymethods]
impl PyCompiledSchema {
    fn get_model_width(&self) -> usize {
        self.inner.token_table().model_width()
    }

    fn __repr__(&self) -> String {
        let plan = self.inner.memory_plan();
        let constraints = plan.unique_items().len()
            + plan.contains_constraints().len()
            + plan.object_extension_count();
        format!(
            "CompiledSchema(model_width={}, constraints={})",
            self.inner.token_table().model_width(),
            constraints
        )
    }
}

#[pyfunction(name = "compile_schema", signature = (schema_json, vocabulary, model_width, *, extensions_json=None))]
fn compile_schema_py(
    py: Python<'_>,
    schema_json: &str,
    vocabulary: &PyVocabulary,
    model_width: usize,
    extensions_json: Option<&str>,
) -> PyResult<PyCompiledSchema> {
    py.detach(|| {
        let extension_plan = extensions_json
            .map(ExtensionPlanV1::from_json)
            .transpose()
            .map_err(|error| CompileError::new_err(error.to_string()))?;
        oc_sidememory::compile_schema(
            schema_json.as_bytes(),
            &vocabulary.0,
            model_width,
            &CompileOptions {
                extension_plan,
                ..CompileOptions::default()
            },
        )
        .map(|compiled| PyCompiledSchema {
            inner: Arc::new(compiled),
        })
        .map_err(|error| CompileError::new_err(error.to_string()))
    })
}

#[pyclass(name = "ImportedMemory", module = "oc_sidememory._native", frozen)]
pub struct PyImportedMemory {
    inner: Arc<ImportedMemory>,
}

#[pymethods]
impl PyImportedMemory {
    #[staticmethod]
    fn from_json(py: Python<'_>, identity: &str, version: &str, json_text: &str) -> PyResult<Self> {
        py.detach(|| {
            ImportedMemory::from_json(identity, version, json_text)
                .map(|inner| Self {
                    inner: Arc::new(inner),
                })
                .map_err(|error| match error {
                    ImportError::Resource(_) => ResourceLimitError::new_err(error.to_string()),
                    _ => PyValueError::new_err(error.to_string()),
                })
        })
    }

    fn get_identity(&self) -> &str {
        self.inner.identity()
    }

    fn get_version(&self) -> &str {
        self.inner.version()
    }

    fn get_set_count(&self) -> usize {
        self.inner.set_count()
    }

    fn __repr__(&self) -> String {
        format!(
            "ImportedMemory(identity={:?}, version={:?}, sets={})",
            self.inner.identity(),
            self.inner.version(),
            self.inner.set_count()
        )
    }
}

#[pyclass(name = "SidememoryGuide", module = "oc_sidememory._native")]
pub struct PySidememoryGuide {
    inner: Guide,
}

#[pymethods]
impl PySidememoryGuide {
    #[new]
    #[pyo3(signature = (compiled, max_rollback=32, *, imports=None))]
    fn __new__(
        compiled: &PyCompiledSchema,
        max_rollback: usize,
        imports: Option<&PyImportedMemory>,
    ) -> PyResult<Self> {
        let options = GuideOptions {
            max_rollback_tokens: max_rollback,
            ..GuideOptions::default()
        };
        let result = match imports {
            Some(imports) => Guide::new_with_imports(
                Arc::clone(&compiled.inner),
                options,
                Arc::clone(&imports.inner),
            ),
            None => Guide::new(Arc::clone(&compiled.inner), options),
        };
        result.map(|inner| Self { inner }).map_err(guide_error)
    }

    fn get_state(&self) -> StateId {
        self.inner.state_id()
    }

    fn get_tokens(&mut self) -> PyResult<Vec<TokenId>> {
        self.inner.allowed_tokens().map_err(guide_error)
    }

    fn get_mask(&mut self) -> PyResult<Vec<u32>> {
        let words = self.inner.model_width().div_ceil(32);
        let mut mask = vec![0; words];
        self.inner.write_mask(&mut mask).map_err(guide_error)?;
        Ok(mask)
    }

    fn fill_mask(&mut self, buffer: &Bound<'_, PyAny>) -> PyResult<()> {
        let view = PyMemoryView::from(buffer)?;
        if view.getattr("readonly")?.extract::<bool>()? {
            return Err(PyTypeError::new_err("mask buffer must be writable"));
        }
        if view.getattr("ndim")?.extract::<usize>()? != 1
            || !view.getattr("c_contiguous")?.extract::<bool>()?
        {
            return Err(PyTypeError::new_err(
                "mask buffer must be one-dimensional and contiguous",
            ));
        }
        let format = view.getattr("format")?.extract::<String>()?;
        if view.getattr("itemsize")?.extract::<usize>()? != 4 || format != "I" {
            return Err(PyTypeError::new_err(
                "mask buffer must use native unsigned 32-bit elements",
            ));
        }
        let words = self.get_mask()?;
        if view.len()? != words.len() {
            return Err(PyValueError::new_err(format!(
                "invalid mask buffer: expected {} words, got {}",
                words.len(),
                view.len()?
            )));
        }
        for (index, word) in words.into_iter().enumerate() {
            view.set_item(index, word)?;
        }
        Ok(())
    }

    fn probe(&mut self, token_id: TokenId) -> PyResult<bool> {
        self.inner
            .probe(token_id)
            .map(|decision| decision == ProbeDecision::Allow)
            .map_err(guide_error)
    }

    fn advance(&mut self, token_id: TokenId) -> PyResult<()> {
        self.inner.advance(token_id).map_err(guide_error)
    }

    #[pyo3(signature = (count=1))]
    fn rollback(&mut self, count: usize) -> PyResult<()> {
        self.inner.rollback(count).map_err(guide_error)
    }

    fn reset(&mut self) -> PyResult<()> {
        self.inner.reset().map_err(guide_error)
    }

    fn is_accepting(&mut self) -> PyResult<bool> {
        self.inner.is_accepting().map_err(guide_error)
    }

    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }

    fn get_allowed_rollback(&self) -> usize {
        self.inner.rollback_available()
    }

    #[pyo3(signature = (tokens, finish=false))]
    fn accepts_tokens(&mut self, tokens: Vec<TokenId>, finish: bool) -> PyResult<bool> {
        self.inner
            .probe_sequence(&tokens, finish)
            .map(|decision| decision == SequenceDecision::Allow)
            .map_err(guide_error)
    }

    fn __reduce__(&self) -> PyResult<()> {
        Err(PyTypeError::new_err(
            "Live SidememoryGuide serialization is not supported in version 0.1. Use the original schema, vocabulary identity, options and token trace.",
        ))
    }
}

fn encode_envelope<T: Encode>(value: &T, kind: u8) -> PyResult<Vec<u8>> {
    let payload = bincode::encode_to_vec(value, config::standard())
        .map_err(|error| PyValueError::new_err(format!("Serialization failed: {error}")))?;
    let mut bytes = Vec::with_capacity(5 + payload.len());
    bytes.extend_from_slice(SERIALIZATION_MAGIC);
    bytes.push(SERIALIZATION_VERSION);
    bytes.push(kind);
    bytes.extend_from_slice(&payload);
    Ok(bytes)
}

fn decode_envelope<T: Decode<()>>(bytes: &[u8], kind: u8) -> PyResult<T> {
    if bytes.len() < 5 || &bytes[..3] != SERIALIZATION_MAGIC {
        return Err(PyValueError::new_err(
            "Unversioned or incompatible serialized data",
        ));
    }
    if bytes[3] != SERIALIZATION_VERSION || bytes[4] != kind {
        return Err(PyValueError::new_err(
            "Unsupported serialization version or object kind",
        ));
    }
    let (value, consumed): (T, usize) = bincode::decode_from_slice(&bytes[5..], config::standard())
        .map_err(|error| PyValueError::new_err(format!("Deserialization failed: {error}")))?;
    if consumed != bytes.len() - 5 {
        return Err(PyValueError::new_err("Trailing bytes in serialized data"));
    }
    Ok(value)
}

macro_rules! type_name {
    ($obj:expr) => {
        // Safety: obj is always initialized and tp_name is a C-string
        unsafe { std::ffi::CStr::from_ptr((&*(&*$obj.as_ptr()).ob_type).tp_name) }
    };
}

/// Guide object based on Index.
#[pyclass(name = "Guide", module = "oc_sidememory._native", from_py_object)]
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct PyGuide {
    state: StateId,
    index: PyIndex,
    state_cache: VecDeque<StateId>,
}

#[pymethods]
impl PyGuide {
    /// Creates a Guide object based on Index.
    #[new]
    #[pyo3(signature = (index, max_rollback=32))]
    fn __new__(index: PyIndex, max_rollback: usize) -> Self {
        PyGuide {
            state: index.get_initial_state(),
            index,
            state_cache: VecDeque::with_capacity(max_rollback),
        }
    }

    /// Retrieves current state id of the Guide.
    fn get_state(&self) -> StateId {
        self.state
    }

    /// Gets the list of allowed tokens for the current state.
    fn get_tokens(&self) -> PyResult<Vec<TokenId>> {
        self.index
            .get_allowed_tokens(self.state)
            // Since Guide advances only through the states offered by the Index, it means
            // None here shouldn't happen and it's an issue at Index creation step
            .ok_or(PyErr::new::<PyValueError, _>(format!(
                "No allowed tokens available for the state {}",
                self.state
            )))
    }

    /// Get the number of rollback steps available.
    fn get_allowed_rollback(&self) -> usize {
        self.state_cache.len()
    }

    /// Guide moves to the next state provided by the token id and returns a list of allowed tokens, unless return_tokens is False.
    #[pyo3(signature = (token_id, return_tokens=None))]
    fn advance(
        &mut self,
        token_id: TokenId,
        return_tokens: Option<bool>,
    ) -> PyResult<Option<Vec<TokenId>>> {
        match self.index.get_next_state(self.state, token_id) {
            Some(new_state) => {
                // Free up space in state_cache if needed.
                if self.state_cache.len() == self.state_cache.capacity() {
                    self.state_cache.pop_front();
                }
                self.state_cache.push_back(self.state);
                self.state = new_state;
                if return_tokens.unwrap_or(true) {
                    self.get_tokens().map(Some)
                } else {
                    Ok(None)
                }
            }
            None => Err(PyErr::new::<PyValueError, _>(format!(
                "No next state found for the current state: {} with token ID: {token_id}",
                self.state
            ))),
        }
    }

    /// Rollback the Guide state `n` tokens (states).
    /// Fails if `n` is greater than stored prior states.
    fn rollback_state(&mut self, n: usize) -> PyResult<()> {
        if n == 0 {
            return Ok(());
        }
        if n > self.get_allowed_rollback() {
            return Err(PyValueError::new_err(format!(
                "Cannot roll back {n} step(s): only {available} states stored (max_rollback = {cap}). \
                 You must advance through at least {n} state(s) before rolling back {n} step(s).",
                 cap = self.state_cache.capacity(),
                 available = self.get_allowed_rollback(),
            )));
        }
        let mut new_state: u32 = self.state;
        for _ in 0..n {
            // unwrap is safe because length is checked above
            new_state = self.state_cache.pop_back().unwrap();
        }
        self.state = new_state;
        Ok(())
    }

    // Returns a boolean indicating if the sequence leads to a valid state in the DFA
    fn accepts_tokens(&self, sequence: Vec<u32>) -> bool {
        let mut state = self.state;
        for t in sequence {
            match self.index.get_next_state(state, t) {
                Some(s) => state = s,
                None => return false,
            }
        }
        true
    }

    /// Checks if the automaton is in a final state.
    fn is_finished(&self) -> bool {
        self.index.is_final_state(self.state)
    }

    /// Write the mask of allowed tokens into the memory specified by data_ptr.
    /// Size of the memory to be written to is indicated by `numel`, and `element_size`.
    /// `element_size` must be 4.
    ///
    /// `data_ptr` should be the data ptr to a `torch.tensor`, or `np.ndarray`, `mx.array` or other
    /// contiguous memory array.
    fn write_mask_into(&self, data_ptr: usize, numel: usize, element_size: usize) -> PyResult<()> {
        let expected_elements = self.index.0.vocab_size().div_ceil(32);
        if element_size != 4 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!(
                    "Invalid element size: got {} bytes per element, expected 4 bytes (32-bit integer).",
                    element_size
                ),
            ));
        } else if data_ptr == 0 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "Invalid data pointer: received a null pointer.",
            ));
        } else if data_ptr % 4 != 0 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(format!(
                "Invalid data pointer alignment: pointer address {} is not a multiple of 4.",
                data_ptr
            )));
        } else if numel < expected_elements {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!(
                    "Invalid buffer size: got {} elements ({} bytes), expected {} elements ({} bytes). \
                    Ensure that the mask tensor has shape (1, (vocab_size + 31) // 32) and uses 32-bit integers.",
                    numel,
                    numel * element_size,
                    expected_elements,
                    expected_elements * 4
                )
            ));
        }
        unsafe {
            std::ptr::write_bytes(data_ptr as *mut u8, 0, numel * 4);
        }
        if let Some(tokens) = self.index.0.allowed_tokens_iter(&self.state) {
            let slice = unsafe { std::slice::from_raw_parts_mut(data_ptr as *mut u32, numel) };
            for &token in tokens {
                let bucket = (token as usize) / 32;
                if bucket < slice.len() {
                    slice[bucket] |= 1 << ((token as usize) % 32);
                }
            }
        }
        Ok(())
    }

    fn reset(&mut self) {
        self.state = self.index.get_initial_state();
    }

    /// Gets the debug string representation of the guide.
    fn __repr__(&self) -> String {
        format!(
            "Guide object with the state={:#?} and {:#?}",
            self.state, self.index
        )
    }

    /// Gets the string representation of the guide.
    fn __str__(&self) -> String {
        format!(
            "Guide object with the state={} and {}",
            self.state, self.index.0
        )
    }

    /// Compares whether two guides are the same.
    fn __eq__(&self, other: &PyGuide) -> bool {
        self == other
    }

    fn __reduce__(&self) -> PyResult<(Py<PyAny>, (Vec<u8>,))> {
        Python::attach(|py| {
            let cls = PyModule::import(py, "oc_sidememory")?.getattr("Guide")?;
            let binary_data = encode_envelope(self, b'G')?;
            Ok((cls.getattr("from_binary")?.unbind(), (binary_data,)))
        })
    }

    #[staticmethod]
    fn from_binary(binary_data: Vec<u8>) -> PyResult<Self> {
        decode_envelope(&binary_data, b'G')
    }
}

/// Index object based on regex and vocabulary.
#[pyclass(
    name = "Index",
    module = "oc_sidememory._native",
    frozen,
    from_py_object
)]
#[derive(Clone, Debug, PartialEq, Encode, Decode)]
pub struct PyIndex(Arc<Index>);

#[pymethods]
impl PyIndex {
    /// Creates an index from a regex and vocabulary.
    #[new]
    fn __new__(py: Python<'_>, regex: &str, vocabulary: &PyVocabulary) -> PyResult<Self> {
        py.detach(|| {
            Index::new(regex, &vocabulary.0)
                .map(|x| PyIndex(Arc::new(x)))
                .map_err(core_error)
        })
    }

    /// Returns allowed tokens in this state.
    fn get_allowed_tokens(&self, state: StateId) -> Option<Vec<TokenId>> {
        self.0.allowed_tokens(&state)
    }

    /// Updates the state.
    fn get_next_state(&self, state: StateId, token_id: TokenId) -> Option<StateId> {
        self.0.next_state(&state, &token_id)
    }

    /// Determines whether the current state is a final state.
    fn is_final_state(&self, state: StateId) -> bool {
        self.0.is_final_state(&state)
    }

    /// Get all final states.
    fn get_final_states(&self) -> HashSet<StateId> {
        self.0.final_states().clone()
    }

    /// Returns the Index as a Python Dict object.
    fn get_transitions(&self) -> HashMap<StateId, HashMap<TokenId, StateId>> {
        self.0.transitions().clone()
    }

    /// Returns the ID of the initial state of the index.
    fn get_initial_state(&self) -> StateId {
        self.0.initial_state()
    }

    /// Gets the debug string representation of the index.
    fn __repr__(&self) -> String {
        format!("{:#?}", self.0)
    }

    /// Gets the string representation of the index.
    fn __str__(&self) -> String {
        format!("{}", self.0)
    }

    /// Compares whether two indexes are the same.
    fn __eq__(&self, other: &PyIndex) -> bool {
        *self.0 == *other.0
    }

    /// Makes a deep copy of the Index.
    fn __deepcopy__(&self, _py: Python<'_>, _memo: Py<PyDict>) -> Self {
        PyIndex(Arc::new((*self.0).clone()))
    }

    fn __reduce__(&self) -> PyResult<(Py<PyAny>, (Vec<u8>,))> {
        Python::attach(|py| {
            let cls = PyModule::import(py, "oc_sidememory")?.getattr("Index")?;
            let binary_data = encode_envelope(self.0.as_ref(), b'I')?;
            Ok((cls.getattr("from_binary")?.unbind(), (binary_data,)))
        })
    }

    #[staticmethod]
    fn from_binary(binary_data: Vec<u8>) -> PyResult<Self> {
        let index: Index = decode_envelope(&binary_data, b'I')?;
        Ok(PyIndex(Arc::new(index)))
    }
}

/// LLM vocabulary.
#[pyclass(name = "Vocabulary", module = "oc_sidememory._native", from_py_object)]
#[derive(Clone, Debug, Encode, Decode)]
pub struct PyVocabulary(Vocabulary);

#[pymethods]
impl PyVocabulary {
    /// Creates a vocabulary from eos token id and a map of tokens to token ids.
    #[new]
    fn __new__(py: Python<'_>, eos_token_id: TokenId, map: Py<PyAny>) -> PyResult<PyVocabulary> {
        if let Ok(dict) = map.extract::<HashMap<String, Vec<TokenId>>>(py) {
            return Vocabulary::try_from((eos_token_id, dict))
                .map(PyVocabulary)
                .map_err(core_error);
        }
        if let Ok(dict) = map.extract::<HashMap<Vec<u8>, Vec<TokenId>>>(py) {
            return Vocabulary::try_from((eos_token_id, dict))
                .map(PyVocabulary)
                .map_err(core_error);
        }

        let message = "Expected a dict with keys of type str or bytes and values of type list[int]";
        let tname = type_name!(map).to_string_lossy();
        if tname == "dict" {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
                "Dict keys or/and values of the wrong types. {message}"
            )))
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
                "{message}, got {tname}"
            )))
        }
    }

    /// Creates the vocabulary of a pre-trained model.
    #[staticmethod]
    #[pyo3(signature = (model, revision=None, token=None))]
    #[cfg(feature = "huggingface-hub")]
    fn from_pretrained(
        model: String,
        revision: Option<String>,
        token: Option<String>,
    ) -> PyResult<PyVocabulary> {
        let mut params = FromPretrainedParameters::default();
        if let Some(r) = revision {
            params.revision = r
        }
        if token.is_some() {
            params.token = token
        }
        let v = Vocabulary::from_pretrained(model.as_str(), Some(params)).map_err(core_error)?;
        Ok(PyVocabulary(v))
    }

    /// Creates a vocabulary from a loaded Transformers fast tokenizer.
    #[staticmethod]
    fn from_transformers(tokenizer: &Bound<'_, PyAny>) -> PyResult<PyVocabulary> {
        let eos_token_id = tokenizer
            .getattr("eos_token_id")?
            .extract::<Option<TokenId>>()?
            .ok_or_else(|| PyValueError::new_err("tokenizer.eos_token_id must be set"))?;
        let backend = tokenizer.getattr("backend_tokenizer")?;
        let json = backend.call_method0("to_str")?.extract::<String>()?;
        let tokenizer = tokenizers::Tokenizer::from_bytes(json.as_bytes())
            .map_err(|error| PyValueError::new_err(error.to_string()))?;
        Vocabulary::from_tokenizer(tokenizer, eos_token_id)
            .map(PyVocabulary)
            .map_err(core_error)
    }

    /// Inserts new token with token_id or extends list of token_ids if token already present.
    fn insert(&mut self, py: Python<'_>, token: Py<PyAny>, token_id: TokenId) -> PyResult<()> {
        if let Ok(t) = token.extract::<String>(py) {
            return self.0.try_insert(t, token_id).map_err(core_error);
        }
        if let Ok(t) = token.extract::<Token>(py) {
            return self.0.try_insert(t, token_id).map_err(core_error);
        }
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
            "Expected a token of type str or bytes, got {:?}",
            type_name!(token)
        )))
    }

    /// Removes a token from vocabulary.
    fn remove(&mut self, py: Python<'_>, token: Py<PyAny>) -> PyResult<()> {
        if let Ok(t) = token.extract::<String>(py) {
            self.0.remove(t);
            return Ok(());
        }
        if let Ok(t) = token.extract::<Token>(py) {
            self.0.remove(t);
            return Ok(());
        }
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
            "Expected a token of type str or bytes, got {:?}",
            type_name!(token)
        )))
    }

    /// Gets token ids of a given token.
    fn get(&self, py: Python<'_>, token: Py<PyAny>) -> PyResult<Option<Vec<TokenId>>> {
        if let Ok(t) = token.extract::<String>(py) {
            return Ok(self.0.token_ids(t.into_bytes()).cloned());
        }
        if let Ok(t) = token.extract::<Token>(py) {
            return Ok(self.0.token_ids(&t).cloned());
        }
        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(format!(
            "Expected a token of type str or bytes, got {:?}",
            type_name!(token)
        )))
    }

    /// Gets the end of sentence token id.
    fn get_eos_token_id(&self) -> TokenId {
        self.0.eos_token_id()
    }

    /// Gets the debug string representation of the vocabulary.
    fn __repr__(&self) -> String {
        format!("{:#?}", self.0)
    }

    /// Gets the string representation of the vocabulary.
    fn __str__(&self) -> String {
        format!("{}", self.0)
    }

    /// Compares whether two vocabularies are the same.
    fn __eq__(&self, other: &PyVocabulary) -> bool {
        self.0 == other.0
    }

    /// Returns length of Vocabulary's tokens, excluding EOS token.
    fn __len__(&self) -> usize {
        self.0.len()
    }

    /// Makes a deep copy of the Vocabulary.
    fn __deepcopy__(&self, _py: Python<'_>, _memo: Py<PyDict>) -> Self {
        PyVocabulary(self.0.clone())
    }

    fn __reduce__(&self) -> PyResult<(Py<PyAny>, (Vec<u8>,))> {
        Python::attach(|py| {
            let cls = PyModule::import(py, "oc_sidememory")?.getattr("Vocabulary")?;
            let binary_data = encode_envelope(self, b'V')?;
            Ok((cls.getattr("from_binary")?.unbind(), (binary_data,)))
        })
    }

    #[staticmethod]
    fn from_binary(binary_data: Vec<u8>) -> PyResult<Self> {
        decode_envelope(&binary_data, b'V')
    }
}

/// Creates regex string from JSON schema with optional whitespace pattern.
#[pyfunction(name = "build_regex_from_schema")]
#[pyo3(signature = (json_schema, whitespace_pattern=None, max_recursion_depth=3))]
pub fn build_regex_from_schema_py(
    json_schema: String,
    whitespace_pattern: Option<&str>,
    max_recursion_depth: usize,
) -> PyResult<String> {
    let value = serde_json::from_str(&json_schema).map_err(|_| {
        PyErr::new::<pyo3::exceptions::PyTypeError, _>("Expected a valid JSON string.")
    })?;
    json_schema::regex_from_value(&value, whitespace_pattern, Some(max_recursion_depth))
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

fn register_child_module(parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    let m = PyModule::new(parent_module.py(), "json_schema")?;
    parent_module.add_submodule(&m)?;

    m.add("BOOLEAN", json_schema::BOOLEAN)?;
    m.add("DATE", json_schema::DATE)?;
    m.add("DATE_TIME", json_schema::DATE_TIME)?;
    m.add("INTEGER", json_schema::INTEGER)?;
    m.add("NULL", json_schema::NULL)?;
    m.add("NUMBER", json_schema::NUMBER)?;
    m.add("STRING", json_schema::STRING)?;
    m.add("STRING_INNER", json_schema::STRING_INNER)?;
    m.add("TIME", json_schema::TIME)?;
    m.add("UUID", json_schema::UUID)?;
    m.add("WHITESPACE", json_schema::WHITESPACE)?;
    m.add("EMAIL", json_schema::EMAIL)?;
    m.add("URI", json_schema::URI)?;
    m.add_function(wrap_pyfunction!(build_regex_from_schema_py, &m)?)?;

    let sys = PyModule::import(m.py(), "sys")?;
    let sys_modules_bind = (sys.as_ref() as &Bound<PyAny>).getattr("modules")?;
    let sys_modules = sys_modules_bind.cast::<PyDict>()?;
    sys_modules.set_item("oc_sidememory.json_schema", &m)?;

    Ok(())
}

/// This package provides core functionality for structured generation, providing a convenient way to:
///
/// - build regular expressions from JSON schemas
///
/// - construct an Index object by combining a Vocabulary and regular expression to efficiently map tokens from a given Vocabulary to state transitions in a finite-state automation
#[pymodule(name = "_native", gil_used = false)]
fn native_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let version = env!("CARGO_PKG_VERSION");
    m.add("__version__", version)?;

    m.add_class::<PyIndex>()?;
    m.add_class::<PyVocabulary>()?;
    m.add_class::<PyGuide>()?;
    m.add_class::<PyCompiledSchema>()?;
    m.add_class::<PyImportedMemory>()?;
    m.add_class::<PySidememoryGuide>()?;
    m.add_function(wrap_pyfunction!(compile_schema_py, m)?)?;
    m.add("OcSidememoryError", m.py().get_type::<OcSidememoryError>())?;
    m.add("CompileError", m.py().get_type::<CompileError>())?;
    m.add(
        "StructuralRejection",
        m.py().get_type::<StructuralRejection>(),
    )?;
    m.add("SemanticViolation", m.py().get_type::<SemanticViolation>())?;
    m.add(
        "ResourceLimitError",
        m.py().get_type::<ResourceLimitError>(),
    )?;
    m.add("LifecycleError", m.py().get_type::<LifecycleError>())?;
    m.add("RollbackError", m.py().get_type::<RollbackError>())?;
    m.add(
        "InternalInvariantError",
        m.py().get_type::<InternalInvariantError>(),
    )?;
    register_child_module(m)?;

    Ok(())
}
