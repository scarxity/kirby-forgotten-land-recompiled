// Shader compilation and reflection for the Vulkan backend (shaders.c).
#pragma once
#include <stddef.h>

#include "vk.h"

typedef struct { uint32_t set, binding; VkDescriptorType type; } VkBindingInfo;

void vk_shaders_init(void);
uint32_t *vk_glsl_to_spirv(const char *src, int stage, const char *name, size_t *nwords);  // malloc'd, NULL on error
// The SPIR-V cache by key (vk_spirv_key: of a source, as vk_glsl_to_spirv stores it): loaded (malloc'd, NULL when
// not cached), or whether it is there
uint64_t vk_spirv_key(const char *src, int stage);
uint32_t *vk_spirv_load(uint64_t key, size_t *nwords);
bool vk_spirv_have(uint64_t key);
VkShaderModule vk_module(const uint32_t *spv, size_t nwords);
// Descriptor bindings declared by a SPIR-V module.
int vk_reflect(const uint32_t *spv, size_t nwords, VkBindingInfo *out, int max);
VkDescriptorSetLayout vk_set_layout(const VkBindingInfo *b, int n, uint32_t set, VkShaderStageFlags stages);
