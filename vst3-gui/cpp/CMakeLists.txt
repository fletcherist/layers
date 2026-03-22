cmake_minimum_required(VERSION 3.15)

if(APPLE)
    project(vst3_gui LANGUAGES CXX OBJCXX)
else()
    project(vst3_gui LANGUAGES CXX)
endif()

set(CMAKE_CXX_STANDARD 17)

if(NOT DEFINED VST3_SDK_PATH)
    message(FATAL_ERROR "VST3_SDK_PATH must be set")
endif()

# Common VST3 SDK sources (cross-platform)
set(VST3_SDK_SOURCES
    ${VST3_SDK_PATH}/public.sdk/source/common/commoniids.cpp
    ${VST3_SDK_PATH}/public.sdk/source/common/memorystream.cpp
    ${VST3_SDK_PATH}/pluginterfaces/base/funknown.cpp
    ${VST3_SDK_PATH}/pluginterfaces/base/coreiids.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/vstinitiids.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/module.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/plugprovider.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/hostclasses.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/connectionproxy.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/pluginterfacesupport.cpp
    ${VST3_SDK_PATH}/public.sdk/source/vst/utility/stringconvert.cpp
    ${VST3_SDK_PATH}/public.sdk/source/common/commonstringconvert.cpp
)

# Platform-specific sources
if(APPLE)
    set(PLATFORM_SDK_SOURCES
        ${VST3_SDK_PATH}/public.sdk/source/common/threadchecker_mac.mm
        ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/module_mac.mm
    )
    set(PLATFORM_SOURCES
        src/vst3_gui_mac.mm
    )
elseif(WIN32)
    set(PLATFORM_SDK_SOURCES
        ${VST3_SDK_PATH}/public.sdk/source/common/threadchecker_win32.cpp
        ${VST3_SDK_PATH}/public.sdk/source/vst/hosting/module_win32.cpp
    )
    set(PLATFORM_SOURCES
        src/vst3_gui_win.cpp
    )
endif()

add_library(vst3_gui STATIC
    src/vst3_gui_common.cpp
    ${PLATFORM_SOURCES}
    ${VST3_SDK_SOURCES}
    ${PLATFORM_SDK_SOURCES}
)

target_include_directories(vst3_gui PUBLIC
    include
    ${VST3_SDK_PATH}
)

# Platform-specific link libraries and compile options
if(APPLE)
    target_link_libraries(vst3_gui PUBLIC
        "-framework AppKit"
        "-framework Cocoa"
        "-framework CoreFoundation"
    )
    target_compile_options(vst3_gui PRIVATE -fobjc-arc)
elseif(WIN32)
    target_link_libraries(vst3_gui PUBLIC
        ole32
        user32
    )
    target_compile_options(vst3_gui PRIVATE /EHsc)
endif()

install(TARGETS vst3_gui ARCHIVE DESTINATION lib)
