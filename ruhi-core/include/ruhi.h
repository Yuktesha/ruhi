/*
 * SPDX-License-Identifier: Apache-2.0
 * ruhi.h — RuhI (如喜・龍龕) Universal Input Method Engine C-ABI Header
 * Compatible with Xcode Swift Bridging Header / XCFramework module.modulemap / cbindgen
 */

#ifndef RUHI_CORE_H
#define RUHI_CORE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/** Opaque pointer to the Rust RuhiEngine instance */
typedef struct RuhiEngine RuhiEngine;

/* === Lifecycle & Dictionary Loading === */
RuhiEngine* ruhi_engine_create(void);
RuhiEngine* ruhi_engine_create_with_dir(const char* path_cstr);
void ruhi_engine_destroy(RuhiEngine* engine);
void ruhi_engine_reset(RuhiEngine* engine);
void ruhi_free_string(char* ptr);

int32_t ruhi_engine_load_tsv(RuhiEngine* engine, const char* path_cstr);
int32_t ruhi_engine_load_symbols_tsv(RuhiEngine* engine, const char* path_cstr);
int32_t ruhi_engine_load_transitions_tsv(RuhiEngine* engine, const char* path_cstr);
int32_t ruhi_engine_load_cangjie_tsv(RuhiEngine* engine, const char* path_cstr);
int32_t ruhi_engine_load_array30_tsv(RuhiEngine* engine, const char* path_cstr);
int32_t ruhi_engine_load_four_corner_tsv(RuhiEngine* engine, const char* path_cstr);

/* === Mode Switching & Keystroke Processing (Returns JSON string; free with ruhi_free_string) === */
int32_t ruhi_engine_set_keyboard_mode(RuhiEngine* engine, const char* mode_cstr);
char* ruhi_engine_process_key(RuhiEngine* engine, char key_code);
char* ruhi_engine_process_key_str(RuhiEngine* engine, const char* key_cstr);

#ifdef __cplusplus
}
#endif

#endif /* RUHI_CORE_H */
