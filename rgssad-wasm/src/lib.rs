use js_sys::ArrayBuffer;
use js_sys::Function;
use js_sys::JsString;
use js_sys::Number;
use js_sys::Uint8Array;
use std::io::Read;
use wasm_bindgen::prelude::*;

/// An Archive Reader
#[wasm_bindgen]
pub struct Reader {
    reader: rgssad::Reader<std::io::Cursor<Vec<u8>>>,
}

#[wasm_bindgen]
impl Reader {
    /// Make a new [`Reader`].
    ///
    /// Accepts either an [`Uint8Array`] or an [`ArrayBuffer`].
    #[wasm_bindgen(constructor)]
    pub fn new(value: &JsValue) -> Result<Reader, JsError> {
        let bytes = value
            .dyn_ref::<Uint8Array>()
            .map(|array| array.to_vec())
            .or_else(|| {
                value
                    .dyn_ref::<ArrayBuffer>()
                    .map(|buffer| Uint8Array::new(buffer).to_vec())
            })
            .ok_or_else(|| JsError::new(&format!("Unknown Argument Type \"{value:?}\"")))?;

        let reader = rgssad::Reader::new(std::io::Cursor::new(bytes))
            .map_err(|error| JsError::new(&error.to_string()))?;

        Ok(Self { reader })
    }

    /// Get the next entry.
    ///
    /// # Arguments
    /// Takes a function as an argument.
    /// This function gets the file name and size as arguments.
    /// If this function returns true, it is skipped.
    /// If this function is absent, the entry is not skipped.
    #[wasm_bindgen(js_name = "readEntry")]
    pub fn read_entry(&mut self, skip: Option<Function>) -> Result<Option<ReaderEntry>, JsValue> {
        loop {
            let entry = self
                .reader
                .read_entry()
                .map_err(|error| JsError::new(&error.to_string()))?;

            let mut entry = match entry {
                Some(entry) => entry,
                None => {
                    return Ok(None);
                }
            };

            let file_name = JsString::from(entry.file_name());
            let size = Number::from(entry.size());

            let should_skip = match skip.as_ref() {
                Some(skip) => skip.call2(&JsValue::NULL, &file_name, &size)?.is_truthy(),
                None => false,
            };

            if should_skip {
                continue;
            }

            let mut buffer = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut buffer)
                .map_err(|error| JsError::new(&error.to_string()))?;
            let data = Uint8Array::new_with_length(entry.size());
            data.copy_from(&buffer);

            return Ok(Some(ReaderEntry { file_name, data }));
        }
    }
}

/// An entry from a [`Reader`].
#[wasm_bindgen]
pub struct ReaderEntry {
    /// The file name
    file_name: JsString,

    /// The file data
    data: Uint8Array,
}

#[wasm_bindgen]
impl ReaderEntry {
    /// Get the file name.
    #[wasm_bindgen(js_name = "fileName", getter)]
    pub fn file_name(&self) -> JsString {
        self.file_name.clone()
    }

    /// Get the file data
    #[wasm_bindgen(getter)]
    pub fn data(&self) -> Uint8Array {
        self.data.clone()
    }
}
