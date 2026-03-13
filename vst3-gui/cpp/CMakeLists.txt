cmake_minimum_required(VERSION 3.15)
project(vst3_gui LANGUAGES CXX OBJCXX)
set(CMAKE_CXX_STANDARD 20)

if(NOT DEFINED VST3_SDK_PATH)
    message(FATAL_ERROR "VST3_SDK_PATH must be set")
endif()

set(VST3_SDK_SOURCES
    ${VST3_SDK_PATH}/public.sdk/source/common/commoniids.cpp
    ${VST3_SDK_PATH}/public.sdk/source/common/memorystream.cpp
    ${VST3_SDK_PATH}/public.sdk/source/common/threadchecker_mac.mm
    ${VST3_SDK_PATH}/pluginterfaces/base/funknown.cpp
    ${VST3_SDK_PATH}/pluginterfaces/base/coreiids.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/vstinitiids.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/module.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/plugprovider.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/hostclasses.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/connectionproxy.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/module_mac.mm
)

add_library(vst3_gui STATIC
    src/vst3_gui.mm
    ${VST3_SDK_SOURCES}
)

target_include_directories(vst3_gui PUBLIC
    include
    ${VST3_SDK_PATH}
)

target_link_libraries(vst3_gui PUBLIC
    "-framework AppKit"
    "-framework Cocoa"
    "-framework CoreFoundation"
)

target_compile_options(vst3_gui PRIVATE -fobjc-arc)

install(TARGETS vst3_gui ARCHIVE DESTINATION lib)
