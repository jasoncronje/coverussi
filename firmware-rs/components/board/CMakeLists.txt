# The board's C and C++ drivers behind a small C API for the Rust firmware
# (src/board.rs): the HUB75 panel, the ES7210 microphones and JPEG decoding.
idf_component_register(SRCS "board.cpp"
                       INCLUDE_DIRS "."
                       REQUIRES driver esp_driver_i2s esp_driver_i2c esp_driver_gpio)
target_compile_features(${COMPONENT_LIB} PUBLIC cxx_std_17)
target_compile_options(${COMPONENT_LIB} PRIVATE -Wno-missing-field-initializers)
