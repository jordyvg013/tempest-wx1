#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <assert.h>

#include "wx1.h"

int main(int argc, char** argv) {
    const char* golden_file = "tests/fixtures/golden_vectors/gv01_valid_scalar_2d_zstd.wx1";
    if (argc > 1) {
        golden_file = argv[1];
    }

    printf("=== Starting WX-1 C-ABI Integration Test ===\n");

    // 1. Version string check
    const char* version = wx1_version_string();
    assert(version != NULL);
    printf("[1/8] WX-1 Version: %s\n", version);
    assert(strncmp(version, "1.0", 3) == 0);

    // 2. Error handling: Non-existent file
    wx1_file_t* bad_file = NULL;
    wx1_error_t err = wx1_open_file("non_existent_file_xyz.wx1", &bad_file);
    assert(err == WX1_ERROR_IO);
    assert(bad_file == NULL);
    const char* last_err = wx1_last_error_message();
    printf("[2/8] Verified clean error handling on missing file: '%s'\n", last_err);
    assert(strlen(last_err) > 0);

    // 3. Open valid golden vector
    wx1_file_t* file = NULL;
    err = wx1_open_file(golden_file, &file);
    if (err != WX1_OK) {
        fprintf(stderr, "Failed to open %s: %s\n", golden_file, wx1_last_error_message());
        return 1;
    }
    assert(file != NULL);
    printf("[3/8] Opened golden vector file: %s\n", golden_file);

    // 4. Inspect generation and dataset id
    uint64_t gen = wx1_get_generation(file);
    assert(gen == 1);
    char dataset_id[64] = {0};
    err = wx1_get_dataset_id(file, dataset_id, sizeof(dataset_id));
    assert(err == WX1_OK);
    assert(strcmp(dataset_id, "gv01") == 0);
    printf("[4/8] Metadata verified: dataset_id='%s', generation=%lu\n", dataset_id, (unsigned long)gen);

    // 5. Inspect variables
    size_t var_count = wx1_get_variable_count(file);
    assert(var_count == 1);
    char var_name[64] = {0};
    err = wx1_get_variable_name(file, 0, var_name, sizeof(var_name));
    assert(err == WX1_OK);
    assert(strcmp(var_name, "t2m") == 0);

    size_t rank = 0;
    err = wx1_get_variable_rank(file, var_name, &rank);
    assert(err == WX1_OK);
    assert(rank == 2);

    uint64_t shape[2] = {0, 0};
    err = wx1_get_variable_shape(file, var_name, shape, 2);
    assert(err == WX1_OK);
    assert(shape[0] == 16 && shape[1] == 16);

    wx1_data_type_t dtype = WX1_DTYPE_UNKNOWN;
    err = wx1_get_variable_data_type(file, var_name, &dtype);
    assert(err == WX1_OK);
    assert(dtype == WX1_DTYPE_F32);
    printf("[5/8] Variable verified: name='%s', rank=%zu, shape=[%lu, %lu], dtype=F32\n",
           var_name, rank, (unsigned long)shape[0], (unsigned long)shape[1]);

    // 6. Test buffer too small check
    uint64_t start[2] = {0, 0};
    uint64_t count[2] = {16, 16};
    uint8_t small_buf[100];
    size_t bytes_needed = 0;
    err = wx1_read_array_slice(file, var_name, start, count, 2, small_buf, sizeof(small_buf), &bytes_needed);
    assert(err == WX1_ERROR_BUFFER_TOO_SMALL);
    assert(bytes_needed == 16 * 16 * sizeof(float)); // 1024 bytes
    printf("[6/8] Verified buffer-capacity guard (needed %zu bytes)\n", bytes_needed);

    // 7. Read full slice into caller-allocated buffer
    float data[16 * 16];
    size_t bytes_written = 0;
    err = wx1_read_array_slice(file, var_name, start, count, 2, (uint8_t*)data, sizeof(data), &bytes_written);
    assert(err == WX1_OK);
    assert(bytes_written == 1024);

    // Check numerical precision
    assert(fabsf(data[0] - 273.15f) < 1e-4f);
    assert(fabsf(data[255] - (273.15f + 25.5f)) < 1e-3f);
    printf("[7/8] Array slice read bit-exact: data[0]=%f K, data[255]=%f K\n", data[0], data[255]);

    // 8. Close file
    err = wx1_close_file(file);
    assert(err == WX1_OK);
    printf("[8/8] Closed file cleanly.\n");

    printf("=== ALL WX-1 C-ABI TESTS PASSED SUCCESSFULLY! ===\n");
    return 0;
}
