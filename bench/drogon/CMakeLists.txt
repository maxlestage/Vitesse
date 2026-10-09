cmake_minimum_required(VERSION 3.10)
project(bench_drogon CXX)

set(CMAKE_CXX_STANDARD 17)
if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

find_package(Drogon CONFIG REQUIRED)
add_executable(bench-drogon main.cc)
target_link_libraries(bench-drogon PRIVATE Drogon::Drogon)
