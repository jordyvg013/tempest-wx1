/*
 * WX-1 Scientific Data Format — C Application Binary Interface (C-ABI)
 * Specification: ADR 0032
 * Copyright (c) 2026 WX-1 Project Contributors
 *
 * This header defines the stable, versioned C-ABI for interacting with WX-1
 * scientific datasets from C, C++, Fortran (iso_c_binding), Julia, and Tempest.
 */

#ifndef WX1_H
#define WX1_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Result status codes returned by WX-1 C-ABI functions.
 */
typedef enum wx1_error {
    WX1_OK = 0,
    WX1_ERROR_NULL_POINTER = 1,
    WX1_ERROR_INVALID_ARGUMENT = 2,
    WX1_ERROR_IO = 3,
    WX1_ERROR_CORRUPT_DATA = 4,
    WX1_ERROR_VARIABLE_NOT_FOUND = 5,
    WX1_ERROR_SLICE_OUT_OF_BOUNDS = 6,
    WX1_ERROR_BUFFER_TOO_SMALL = 7,
    WX1_ERROR_PANIC = 8
} wx1_error_t;

/**
 * Supported primitive scientific array data types in WX-1.
 */
typedef enum wx1_data_type {
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
    WX1_DTYPE_BOOL8 = 11
} wx1_data_type_t;

/**
 * Opaque handle representing an open WX-1 file container and snapshot.
 */
typedef struct wx1_file_t wx1_file_t;

/**
 * Returns a human-readable description of the last error on the calling thread.
 */
const char* wx1_last_error_message(void);

/**
 * Returns the static WX-1 library version string.
 */
const char* wx1_version_string(void);

/**
 * Opens a WX-1 scientific container file.
 *
 * @param path Null-terminated UTF-8 file path.
 * @param out_file Pointer receiving the allocated wx1_file_t handle.
 * @return WX1_OK on success, or an error code on failure.
 */
wx1_error_t wx1_open_file(const char* path, wx1_file_t** out_file);

/**
 * Closes an open WX-1 file handle and releases all internal resources.
 *
 * @param file The file handle to close.
 * @return WX1_OK on success, or an error code.
 */
wx1_error_t wx1_close_file(wx1_file_t* file);

/**
 * Returns the active generation sequence number of the file.
 */
uint64_t wx1_get_generation(const wx1_file_t* file);

/**
 * Copies the dataset identifier into caller-allocated buffer `out_buf`.
 *
 * @param file The open file handle.
 * @param out_buf Destination character buffer.
 * @param buf_len Capacity of out_buf including null terminator.
 * @return WX1_OK on success, or WX1_ERROR_BUFFER_TOO_SMALL.
 */
wx1_error_t wx1_get_dataset_id(const wx1_file_t* file, char* out_buf, size_t buf_len);

/**
 * Returns the total number of variables defined in the dataset.
 */
size_t wx1_get_variable_count(const wx1_file_t* file);

/**
 * Copies the name of the variable at the specified index into `out_buf`.
 *
 * @param file The open file handle.
 * @param index 0-based variable index.
 * @param out_buf Destination character buffer.
 * @param buf_len Capacity of out_buf including null terminator.
 * @return WX1_OK on success, or an error code.
 */
wx1_error_t wx1_get_variable_name(const wx1_file_t* file, size_t index, char* out_buf, size_t buf_len);

/**
 * Queries the array rank (number of dimensions) of the specified variable.
 *
 * @param file The open file handle.
 * @param var_name Variable name or path (e.g., "t2m" or "/t2m").
 * @param out_rank Pointer receiving the rank.
 * @return WX1_OK on success, or WX1_ERROR_VARIABLE_NOT_FOUND.
 */
wx1_error_t wx1_get_variable_rank(const wx1_file_t* file, const char* var_name, size_t* out_rank);

/**
 * Queries the dimension lengths (shape) of the specified variable.
 *
 * @param file The open file handle.
 * @param var_name Variable name or path.
 * @param out_shape Array of uint64_t with length equal to rank.
 * @param rank The rank queried via wx1_get_variable_rank.
 * @return WX1_OK on success, or an error code.
 */
wx1_error_t wx1_get_variable_shape(const wx1_file_t* file, const char* var_name, uint64_t* out_shape, size_t rank);

/**
 * Queries the data type of the specified variable.
 *
 * @param file The open file handle.
 * @param var_name Variable name or path.
 * @param out_dtype Pointer receiving the wx1_data_type_t code.
 * @return WX1_OK on success, or an error code.
 */
wx1_error_t wx1_get_variable_data_type(const wx1_file_t* file, const char* var_name, wx1_data_type_t* out_dtype);

/**
 * Reads a multidimensional array slice directly into caller-allocated memory.
 *
 * @param file The open file handle.
 * @param var_name Variable name or path.
 * @param start Array of starting offsets along each dimension (length = rank).
 * @param count Array of element counts along each dimension (length = rank).
 * @param rank Number of dimensions.
 * @param out_buffer Caller-allocated memory receiving decoded data in C row-major layout.
 * @param buffer_len Total size of out_buffer in bytes.
 * @param out_bytes_written Pointer receiving the exact number of bytes written.
 *                          If buffer_len is too small, receives the required number of bytes.
 * @return WX1_OK on success, WX1_ERROR_BUFFER_TOO_SMALL, or other error code.
 */
wx1_error_t wx1_read_array_slice(
    wx1_file_t* file,
    const char* var_name,
    const uint64_t* start,
    const uint64_t* count,
    size_t rank,
    uint8_t* out_buffer,
    size_t buffer_len,
    size_t* out_bytes_written
);

#ifdef __cplusplus
}
#endif

#endif /* WX1_H */
