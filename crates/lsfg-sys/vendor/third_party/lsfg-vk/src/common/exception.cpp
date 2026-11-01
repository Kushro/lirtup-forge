#include "common/exception.hpp"

#include <vulkan/vulkan_core.h>

#include <exception>
#include <stdexcept>
#include <cstdint>
#include <string>

// LirtUp Forge modification: string concatenation instead of std::format,
// which needs GCC 13 and is missing from older wheel build images.

using namespace LSFG;

vulkan_error::vulkan_error(VkResult result, const std::string& message)
    : std::runtime_error(message + " (error " +
          std::to_string(static_cast<int32_t>(result)) + ")"),
      result(result) {}

vulkan_error::~vulkan_error() noexcept = default;

rethrowable_error::rethrowable_error(const std::string& message, const std::exception& exe)
        : std::runtime_error(message) {
    this->message = message + "\n- " + exe.what();
}

rethrowable_error::~rethrowable_error() noexcept = default;
