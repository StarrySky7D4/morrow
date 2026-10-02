cmake_minimum_required(VERSION 3.13)
project(runner LANGUAGES CXX)

add_executable(${BINARY_NAME}
  "main.cc"
  "my_application.cc"
  "launch_policy.cc"
)
apply_standard_settings(${BINARY_NAME})
target_compile_definitions(${BINARY_NAME} PRIVATE
  APPLICATION_ID="${APPLICATION_ID}")
target_link_libraries(${BINARY_NAME} PRIVATE PkgConfig::GTK)
# No Flutter engine or plugin registration until protected product wiring is
# complete. Merely building the foundation cannot enable the legacy Dart path.
