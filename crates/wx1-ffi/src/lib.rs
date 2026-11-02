#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};
use std::fs::File;
use std::panic::catch_unwind;
use std::path::Path;

use wx1_format::{open_scientific_snapshot, ArraySelection, DataType, ScientificSnapshot};

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum wx1_error_t {
    WX1_OK = 0,
    WX1_ERROR_NULL_POINTER = 1,
    WX1_ERROR_INVALID_ARGUMENT = 2,
    WX1_ERROR_IO = 3,
    WX1_ERROR_CORRUPT_DATA = 4,
    WX1_ERROR_VARIABLE_NOT_FOUND = 5,
    WX1_ERROR_SLICE_OUT_OF_BOUNDS = 6,
    WX1_ERROR_BUFFER_TOO_SMALL = 7,
    WX1_ERROR_PANIC = 8,
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum wx1_data_type_t {
    WX1_DTYPE_UNKNOWN = 0,
    WX1_DTYPE_U8 = 1,
    WX1_DTYPE_I8 = 2,
    WX1_DTYPE_U16 = 3,
    WX1_DTYPE_I16 = 4,
    WX1_DTYPE_U32 = 5,
    WX1_DTYPE_I32 = 6,
    WX1_DTYPE_U64 = 7,
    WX1_DTYPE_I64 = 8,
    WX1_DTYPE_F32 = 9,
    WX1_DTYPE_F64 = 10,
    WX1_DTYPE_BOOL8 = 11,
}

impl From<DataType> for wx1_data_type_t {
    fn from(dtype: DataType) -> Self {
        match dtype {
            DataType::U8 => Self::WX1_DTYPE_U8,
            DataType::I8 => Self::WX1_DTYPE_I8,
            DataType::U16 => Self::WX1_DTYPE_U16,
            DataType::I16 => Self::WX1_DTYPE_I16,
            DataType::U32 => Self::WX1_DTYPE_U32,
            DataType::I32 => Self::WX1_DTYPE_I32,
            DataType::U64 => Self::WX1_DTYPE_U64,
            DataType::I64 => Self::WX1_DTYPE_I64,
            DataType::F32 => Self::WX1_DTYPE_F32,
            DataType::F64 => Self::WX1_DTYPE_F64,
            DataType::Bool8 => Self::WX1_DTYPE_BOOL8,
        }
    }
}

pub struct wx1_file_t {
    file: File,
    snapshot: ScientificSnapshot,
}

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_last_error(msg: impl AsRef<str>) {
    let clean = msg.as_ref().replace('\0', " ");
    let c_str = CString::new(clean).unwrap_or_default();
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = c_str;
    });
}

fn copy_to_c_buffer(src: &str, out_buf: *mut c_char, buf_len: usize) -> wx1_error_t {
    if out_buf.is_null() || buf_len == 0 {
        return wx1_error_t::WX1_ERROR_NULL_POINTER;
    }
    let src_bytes = src.as_bytes();
    if src_bytes.len() >= buf_len {
        return wx1_error_t::WX1_ERROR_BUFFER_TOO_SMALL;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(src_bytes.as_ptr(), out_buf as *mut u8, src_bytes.len());
        *out_buf.add(src_bytes.len()) = 0;
    }
    wx1_error_t::WX1_OK
}

/// Returns a human-readable description of the last error that occurred on the calling thread.
#[no_mangle]
pub extern "C" fn wx1_last_error_message() -> *const c_char {
    LAST_ERROR.with(|cell| cell.borrow().as_ptr())
}

/// Returns the static WX-1 library version string.
#[no_mangle]
pub extern "C" fn wx1_version_string() -> *const c_char {
    static VERSION: &[u8] = b"1.0.0-rc1\0";
    VERSION.as_ptr() as *const c_char
}

/// Opens a WX-1 scientific container file.
#[no_mangle]
pub unsafe extern "C" fn wx1_open_file(
    path: *const c_char,
    out_file: *mut *mut wx1_file_t,
) -> wx1_error_t {
    let res = catch_unwind(|| {
        if path.is_null() || out_file.is_null() {
            set_last_error("null pointer passed to wx1_open_file");
            return wx1_error_t::WX1_ERROR_NULL_POINTER;
        }

        let c_str = match CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("path is not valid UTF-8");
                return wx1_error_t::WX1_ERROR_INVALID_ARGUMENT;
            }
        };

        let mut file = match File::open(Path::new(c_str)) {
            Ok(f) => f,
            Err(e) => {
                set_last_error(format!("failed to open file '{c_str}': {e}"));
                return wx1_error_t::WX1_ERROR_IO;
            }
        };

        let snapshot = match open_scientific_snapshot(&mut file) {
            Ok(s) => s,
            Err(e) => {
                set_last_error(format!("failed to open WX-1 snapshot: {e}"));
                return wx1_error_t::WX1_ERROR_CORRUPT_DATA;
            }
        };

        let handle = Box::into_raw(Box::new(wx1_file_t { file, snapshot }));
        *out_file = handle;
        wx1_error_t::WX1_OK
    });

    res.unwrap_or_else(|_| {
        set_last_error("panic occurred inside wx1_open_file");
        wx1_error_t::WX1_ERROR_PANIC
    })
}

/// Closes a previously opened WX-1 file container and frees its associated resources.
#[no_mangle]
pub unsafe extern "C" fn wx1_close_file(file: *mut wx1_file_t) -> wx1_error_t {
    let res = catch_unwind(|| {
        if file.is_null() {
            set_last_error("null pointer passed to wx1_close_file");
            return wx1_error_t::WX1_ERROR_NULL_POINTER;
        }
        drop(Box::from_raw(file));
        wx1_error_t::WX1_OK
    });

    res.unwrap_or_else(|_| {
        set_last_error("panic occurred inside wx1_close_file");
        wx1_error_t::WX1_ERROR_PANIC
    })
}

/// Gets the active committed generation number of the container.
#[no_mangle]
pub unsafe extern "C" fn wx1_get_generation(file: *const wx1_file_t) -> u64 {
    if file.is_null() {
        return 0;
    }
    (*file).snapshot.generation()
}

/// Copies the dataset identifier into caller-allocated buffer `out_buf`.
#[no_mangle]
pub unsafe extern "C" fn wx1_get_dataset_id(
    file: *const wx1_file_t,
    out_buf: *mut c_char,
    buf_len: usize,
) -> wx1_error_t {
    let res = catch_unwind(|| {
        if file.is_null() {
            set_last_error("null file pointer passed to wx1_get_dataset_id");
            return wx1_error_t::WX1_ERROR_NULL_POINTER;
        }
        let id = &(*file).snapshot.metadata().dataset_id;
        copy_to_c_buffer(id, out_buf, buf_len)
    });

    res.unwrap_or(wx1_error_t::WX1_ERROR_PANIC)
}

/// Gets the number of scientific variables in the dataset.
#[no_mangle]
pub unsafe extern "C" fn wx1_get_variable_count(file: *const wx1_file_t) -> usize {
    if file.is_null() {
        return 0;
    }
    (*file).snapshot.dataset().variables.len()
}

/// Copies the name of the variable at `index` into caller-allocated buffer `out_buf`.
#[no_mangle]
pub unsafe extern "C" fn wx1_get_variable_name(
    file: *const wx1_file_t,
    index: usize,
    out_buf: *mut c_char,
    buf_len: usize,
) -> wx1_error_t {
    let res = catch_unwind(|| {
        if file.is_null() {
            set_last_error("null file pointer passed to wx1_get_variable_name");
            return wx1_error_t::WX1_ERROR_NULL_POINTER;
        }
        let vars = &(*file).snapshot.dataset().variables;
        if index >= vars.len() {
            set_last_error(format!(
                "variable index {index} out of range (count: {})",
                vars.len()
            ));
            return wx1_error_t::WX1_ERROR_INVALID_ARGUMENT;
        }
        copy_to_c_buffer(&vars[index].name, out_buf, buf_len)
    });

    res.unwrap_or(wx1_error_t::WX1_ERROR_PANIC)
}

/// Gets the array rank (number of dimensions) of a variable.
#[no_mangle]
pub unsafe extern "C" fn wx1_get_variable_rank(
    file: *const wx1_file_t,
    var_name: *const c_char,
    out_rank: *mut usize,
) -> wx1_error_t {
    let res = catch_unwind(|| {
        if file.is_null() || var_name.is_null() || out_rank.is_null() {
            set_last_error("null pointer passed to wx1_get_variable_rank");
            return wx1_error_t::WX1_ERROR_NULL_POINTER;
        }
        let name_str = match CStr::from_ptr(var_name).to_str() {
            Ok(s) => s.trim_start_matches('/'),
            Err(_) => return wx1_error_t::WX1_ERROR_INVALID_ARGUMENT,
        };
        let var = (*file)
            .snapshot
            .dataset()
            .variables
            .iter()
            .find(|v| v.name == name_str);
        match var {
            Some(v) => {
                *out_rank = v.dimensions.len();
                wx1_error_t::WX1_OK
            }
            None => {
                set_last_error(format!("variable '{name_str}' not found"));
                wx1_error_t::WX1_ERROR_VARIABLE_NOT_FOUND
            }
        }
    });

    res.unwrap_or(wx1_error_t::WX1_ERROR_PANIC)
}

/// Gets the shape (dimension lengths) of a variable into caller-allocated buffer `out_shape`.
#[no_mangle]
pub unsafe extern "C" fn wx1_get_variable_shape(
    file: *const wx1_file_t,
    var_name: *const c_char,
    out_shape: *mut u64,
    rank: usize,
) -> wx1_error_t {
    let res = catch_unwind(|| {
        if file.is_null() || var_name.is_null() || out_shape.is_null() {
            set_last_error("null pointer passed to wx1_get_variable_shape");
            return wx1_error_t::WX1_ERROR_NULL_POINTER;
        }
        let name_str = match CStr::from_ptr(var_name).to_str() {
            Ok(s) => s.trim_start_matches('/'),
            Err(_) => return wx1_error_t::WX1_ERROR_INVALID_ARGUMENT,
        };
        let dataset = (*file).snapshot.dataset();
        let var = match dataset.variables.iter().find(|v| v.name == name_str) {
            Some(v) => v,
            None => return wx1_error_t::WX1_ERROR_VARIABLE_NOT_FOUND,
        };
        if var.dimensions.len() != rank {
            set_last_error(format!(
                "rank mismatch: variable rank is {}, caller provided buffer for {}",
                var.dimensions.len(),
                rank
            ));
            return wx1_error_t::WX1_ERROR_INVALID_ARGUMENT;
        }
        for (i, dim_name) in var.dimensions.iter().enumerate() {
            let dim = match dataset.dimensions.iter().find(|d| &d.name == dim_name) {
                Some(d) => d,
                None => return wx1_error_t::WX1_ERROR_CORRUPT_DATA,
            };
            *out_shape.add(i) = dim.length;
        }
        wx1_error_t::WX1_OK
    });

    res.unwrap_or(wx1_error_t::WX1_ERROR_PANIC)
}

/// Gets the data type of a variable.
#[no_mangle]
pub unsafe extern "C" fn wx1_get_variable_data_type(
    file: *const wx1_file_t,
    var_name: *const c_char,
    out_dtype: *mut wx1_data_type_t,
) -> wx1_error_t {
    let res = catch_unwind(|| {
        if file.is_null() || var_name.is_null() || out_dtype.is_null() {
            return wx1_error_t::WX1_ERROR_NULL_POINTER;
        }
        let name_str = match CStr::from_ptr(var_name).to_str() {
            Ok(s) => s.trim_start_matches('/'),
            Err(_) => return wx1_error_t::WX1_ERROR_INVALID_ARGUMENT,
        };
        let var = match (*file)
            .snapshot
            .dataset()
            .variables
            .iter()
            .find(|v| v.name == name_str)
        {
            Some(v) => v,
            None => return wx1_error_t::WX1_ERROR_VARIABLE_NOT_FOUND,
        };
        *out_dtype = var.data_type.into();
        wx1_error_t::WX1_OK
    });

    res.unwrap_or(wx1_error_t::WX1_ERROR_PANIC)
}

/// Reads a multidimensional hyperslab slice of a variable directly into caller's buffer `out_buffer`.
#[no_mangle]
pub unsafe extern "C" fn wx1_read_array_slice(
    file: *mut wx1_file_t,
    var_name: *const c_char,
    start: *const u64,
    count: *const u64,
    rank: usize,
    out_buffer: *mut u8,
    buffer_len: usize,
    out_bytes_written: *mut usize,
) -> wx1_error_t {
    let res = catch_unwind(std::panic::AssertUnwindSafe(|| {
        if file.is_null()
            || var_name.is_null()
            || start.is_null()
            || count.is_null()
            || out_buffer.is_null()
            || out_bytes_written.is_null()
        {
            set_last_error("null pointer argument in wx1_read_array_slice");
            return wx1_error_t::WX1_ERROR_NULL_POINTER;
        }

        let name_str = match CStr::from_ptr(var_name).to_str() {
            Ok(s) => {
                if s.starts_with('/') {
                    s.to_owned()
                } else {
                    format!("/{s}")
                }
            }
            Err(_) => return wx1_error_t::WX1_ERROR_INVALID_ARGUMENT,
        };

        let start_slice = std::slice::from_raw_parts(start, rank);
        let count_slice = std::slice::from_raw_parts(count, rank);

        let selection = ArraySelection {
            start: start_slice.to_vec(),
            count: count_slice.to_vec(),
        };

        let handle = &mut *file;
        let array = match handle
            .snapshot
            .read_array_slice(&mut handle.file, &name_str, &selection)
        {
            Ok(a) => a,
            Err(e) => {
                set_last_error(format!("failed to read array slice '{name_str}': {e}"));
                return wx1_error_t::WX1_ERROR_SLICE_OUT_OF_BOUNDS;
            }
        };

        let data_len = array.data.len();
        *out_bytes_written = data_len;

        if buffer_len < data_len {
            set_last_error(format!(
                "buffer too small: need {data_len} bytes, got {buffer_len}"
            ));
            return wx1_error_t::WX1_ERROR_BUFFER_TOO_SMALL;
        }

        std::ptr::copy_nonoverlapping(array.data.as_ptr(), out_buffer, data_len);
        wx1_error_t::WX1_OK
    }));

    res.unwrap_or_else(|_| {
        set_last_error("panic occurred in wx1_read_array_slice");
        wx1_error_t::WX1_ERROR_PANIC
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_c_abi_version_string() {
        let v = wx1_version_string();
        assert!(!v.is_null());
        let s = unsafe { CStr::from_ptr(v).to_str().unwrap() };
        assert!(s.starts_with("1.0.0"));
    }

    #[test]
    fn test_c_abi_open_close_and_slice() {
        let path = "../../tests/fixtures/golden_vectors/gv01_valid_scalar_2d_zstd.wx1";
        let c_path = CString::new(path).unwrap();

        let mut file_ptr: *mut wx1_file_t = std::ptr::null_mut();
        let err = unsafe { wx1_open_file(c_path.as_ptr(), &mut file_ptr) };
        assert_eq!(err, wx1_error_t::WX1_OK);
        assert!(!file_ptr.is_null());

        let gen = unsafe { wx1_get_generation(file_ptr) };
        assert_eq!(gen, 1);

        let var_count = unsafe { wx1_get_variable_count(file_ptr) };
        assert_eq!(var_count, 1);

        let mut name_buf = [0 as c_char; 64];
        let err = unsafe { wx1_get_variable_name(file_ptr, 0, name_buf.as_mut_ptr(), 64) };
        assert_eq!(err, wx1_error_t::WX1_OK);
        let name_str = unsafe { CStr::from_ptr(name_buf.as_ptr()).to_str().unwrap() };
        assert_eq!(name_str, "t2m");

        let c_var = CString::new("t2m").unwrap();
        let mut rank = 0usize;
        let err = unsafe { wx1_get_variable_rank(file_ptr, c_var.as_ptr(), &mut rank) };
        assert_eq!(err, wx1_error_t::WX1_OK);
        assert_eq!(rank, 2);

        let mut shape = [0u64; 2];
        let err =
            unsafe { wx1_get_variable_shape(file_ptr, c_var.as_ptr(), shape.as_mut_ptr(), 2) };
        assert_eq!(err, wx1_error_t::WX1_OK);
        assert_eq!(shape, [16, 16]);

        let mut dtype = wx1_data_type_t::WX1_DTYPE_UNKNOWN;
        let err = unsafe { wx1_get_variable_data_type(file_ptr, c_var.as_ptr(), &mut dtype) };
        assert_eq!(err, wx1_error_t::WX1_OK);
        assert_eq!(dtype, wx1_data_type_t::WX1_DTYPE_F32);

        // Read 16x16 slice (256 floats = 1024 bytes)
        let start = [0u64, 0u64];
        let count = [16u64, 16u64];
        let mut buffer = vec![0u8; 1024];
        let mut bytes_written = 0usize;

        let err = unsafe {
            wx1_read_array_slice(
                file_ptr,
                c_var.as_ptr(),
                start.as_ptr(),
                count.as_ptr(),
                2,
                buffer.as_mut_ptr(),
                1024,
                &mut bytes_written,
            )
        };
        assert_eq!(err, wx1_error_t::WX1_OK);
        assert_eq!(bytes_written, 1024);

        let (chunks, _) = buffer.as_chunks::<4>();
        let f32_vals: Vec<f32> = chunks.iter().map(|b| f32::from_le_bytes(*b)).collect();
        assert_eq!(f32_vals.len(), 256);
        assert!((f32_vals[0] - 273.15).abs() < 1e-4);

        // Test buffer too small error handling
        let mut small_buffer = vec![0u8; 500];
        let mut needed_bytes = 0usize;
        let err = unsafe {
            wx1_read_array_slice(
                file_ptr,
                c_var.as_ptr(),
                start.as_ptr(),
                count.as_ptr(),
                2,
                small_buffer.as_mut_ptr(),
                500,
                &mut needed_bytes,
            )
        };
        assert_eq!(err, wx1_error_t::WX1_ERROR_BUFFER_TOO_SMALL);
        assert_eq!(needed_bytes, 1024);

        let err = unsafe { wx1_close_file(file_ptr) };
        assert_eq!(err, wx1_error_t::WX1_OK);
    }
}
