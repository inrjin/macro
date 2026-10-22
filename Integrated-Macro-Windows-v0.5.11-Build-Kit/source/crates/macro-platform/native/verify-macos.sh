#!/bin/bash
set -euo pipefail

# Does not request Screen Recording/Accessibility permission or send input.
native_dir="$(cd "$(dirname "$0")" && pwd)"
check_dir="$(mktemp -d "${TMPDIR:-/tmp}/macro-native-check.XXXXXX")"
trap 'rm -rf "$check_dir"' EXIT
sdk="$(xcrun --show-sdk-path)"
swift_binary="$(xcrun --find swiftc)"
swift_root="$(dirname "$(dirname "$swift_binary")")"

for arch in arm64 x86_64; do
    xcrun swiftc -swift-version 5 -O -parse-as-library -emit-library -static \
        -module-name MacroNative -D MACRO_NATIVE_SELF_TEST -target "$arch-apple-macosx13.0" -sdk "$sdk" \
        -module-cache-path "$check_dir/cache-$arch" "$native_dir/MacPlatform.swift" \
        -o "$check_dir/libMacroNative-$arch.a"
done

cat > "$check_dir/smoke.c" <<'EOF'
#include <stdint.h>
#include <stdio.h>
#include <string.h>
extern uint32_t mmp_permissions(void);
extern uint32_t mmp_keyboard_input_status(void);
extern char *mmp_keyboard_input_status_test(void);
extern char *mmp_input_cancellation_test(void);
typedef int32_t (*input_cancelled_fn)(const void *);
extern char *mmp_click_guarded(double, double, input_cancelled_fn, const void *, uint64_t *);
extern char *mmp_key_guarded(uint16_t, input_cancelled_fn, const void *, uint64_t *);
extern void *mmp_capture_new(void);
extern void mmp_capture_free(void *);
extern char *mmp_capture_start(void *, uint64_t);
extern char *mmp_capture_lifecycle_test(void);
extern char *mmp_window_validation_test(void);
extern char *mmp_activation_test(void);
extern char *mmp_enter_filter_test(void);
extern void *mmp_enter_new(void);
extern void mmp_enter_free(void *);
extern char *mmp_enter_watch(void *, int32_t, uint64_t);
extern char *mmp_enter_take(void *, uint64_t *, uint64_t, uint64_t *, uint64_t *);
extern char *mmp_enter_diagnostics_json(void *);
extern char *mmp_captcha_input_filter_test(void);
extern char *mmp_captcha_input_callback_test(void);
extern char *mmp_global_monitor_recipient_test(const char *);
extern void *mmp_captcha_input_new(void);
extern void mmp_captcha_input_free(void *);
extern char *mmp_captcha_input_watch(void *, int32_t, uint64_t);
extern char *mmp_captcha_input_take(void *, uint64_t *, uint64_t, uint64_t *, uint64_t *);
extern char *mmp_captcha_input_diagnostics_json(void *);
extern void mmp_free_string(char *);
static int32_t always_cancelled(const void *context) {
    unsigned *calls = (unsigned *)context;
    ++*calls;
    return 1;
}
int main(int argc, char **argv) {
    char *cancel_failure = mmp_input_cancellation_test();
    if (cancel_failure) {
        fprintf(stderr, "%s\n", cancel_failure);
        mmp_free_string(cancel_failure);
        return 22;
    }
    /* Cancelled before any permission query, event creation, or actual input. */
    unsigned cancellation_calls = 0;
    uint64_t click_started = 99, key_started = 99;
    char *cancelled_click = mmp_click_guarded(0, 0, always_cancelled, &cancellation_calls, &click_started);
    char *cancelled_key = mmp_key_guarded(53, always_cancelled, &cancellation_calls, &key_started);
    if (!cancelled_click || !cancelled_key || cancellation_calls != 2
        || click_started != 0 || key_started != 0
        || strncmp(cancelled_click, "input_cancelled|", 16) != 0
        || strncmp(cancelled_key, "input_cancelled|", 16) != 0) return 23;
    mmp_free_string(cancelled_click);
    mmp_free_string(cancelled_key);
    if (argc != 2) return 20;
    char *recipient_failure = mmp_global_monitor_recipient_test(argv[1]);
    if (recipient_failure) {
        fprintf(stderr, "%s\n", recipient_failure);
        mmp_free_string(recipient_failure);
        return 21;
    }
    char *keyboard_status_failure = mmp_keyboard_input_status_test();
    if (keyboard_status_failure) {
        fprintf(stderr, "%s\n", keyboard_status_failure);
        mmp_free_string(keyboard_status_failure);
        return 18;
    }
    /* Read-only ABI check; either permission value is valid in this process. */
    if ((mmp_keyboard_input_status() & ~UINT32_C(7)) != 0) return 19;
    char *enter_failure = mmp_enter_filter_test();
    if (enter_failure) {
        fprintf(stderr, "%s\n", enter_failure);
        mmp_free_string(enter_failure);
        return 7;
    }
    /* Off-only ABI check: no global keyboard monitor is installed. */
    void *enter = mmp_enter_new();
    if (!enter) return 8;
    uint64_t entries[16 * 4] = {0}, count = 99, observed_now = 0;
    enter_failure = mmp_enter_watch(enter, 0, 0);
    if (!enter_failure) enter_failure = mmp_enter_take(enter, entries, 16, &count, &observed_now);
    char *diagnostics = mmp_enter_diagnostics_json(enter);
    if (diagnostics) {
        fprintf(stderr, "Expected empty Enter diagnostic queue, got %s\n", diagnostics);
        mmp_free_string(diagnostics);
        mmp_enter_free(enter);
        return 11;
    }
    mmp_enter_free(enter);
    if (enter_failure) {
        fprintf(stderr, "%s\n", enter_failure);
        mmp_free_string(enter_failure);
        return 9;
    }
    if (count != 0 || observed_now == 0) return 10;
    char *captcha_failure = mmp_captcha_input_filter_test();
    if (captcha_failure) {
        fprintf(stderr, "%s\n", captcha_failure);
        mmp_free_string(captcha_failure);
        return 12;
    }
    /* Synthetic NSEvent callback/queue path, without an OS monitor or input. */
    captcha_failure = mmp_captcha_input_callback_test();
    if (captcha_failure) {
        fprintf(stderr, "%s\n", captcha_failure);
        mmp_free_string(captcha_failure);
        return 16;
    }
    void *captcha = mmp_captcha_input_new();
    if (!captcha) return 13;
    count = 99;
    observed_now = 0;
    captcha_failure = mmp_captcha_input_watch(captcha, 0, 0);
    if (!captcha_failure) captcha_failure = mmp_captcha_input_take(captcha, entries, 16, &count, &observed_now);
    diagnostics = mmp_captcha_input_diagnostics_json(captcha);
    if (diagnostics) {
        fprintf(stderr, "Expected empty CAPTCHA input diagnostic queue, got %s\n", diagnostics);
        mmp_free_string(diagnostics);
        if (captcha_failure) mmp_free_string(captcha_failure);
        mmp_captcha_input_free(captcha);
        return 17;
    }
    mmp_captcha_input_free(captcha);
    if (captcha_failure) {
        fprintf(stderr, "%s\n", captcha_failure);
        mmp_free_string(captcha_failure);
        return 14;
    }
    if (count != 0 || observed_now == 0) return 15;
    char *activation_failure = mmp_activation_test();
    if (activation_failure) {
        fprintf(stderr, "%s\n", activation_failure);
        mmp_free_string(activation_failure);
        return 6;
    }
    char *window_failure = mmp_window_validation_test();
    if (window_failure) {
        fprintf(stderr, "%s\n", window_failure);
        mmp_free_string(window_failure);
        return 5;
    }
    char *test_failure = mmp_capture_lifecycle_test();
    if (test_failure) {
        fprintf(stderr, "%s\n", test_failure);
        mmp_free_string(test_failure);
        return 4;
    }
    void *capture = mmp_capture_new();
    if (!capture) return 2;
    /* Main-thread rejection occurs before permission checking or capture. */
    char *failure = mmp_capture_start(capture, 0);
    if (!failure || strncmp(failure, "capture_thread|", 15) != 0) return 3;
    mmp_free_string(failure);
    mmp_capture_free(capture);
    printf("Native synthetic window validation (43), activation (16), capture lifecycle, Enter/CAPTCHA filters, nine observed recipient-pair replays for both watches, independent watches, read-only keyboard status, diagnostics/off-only ABI, allocation/free and thread guard OK; permission bits=%u\n", mmp_permissions());
    return 0;
}
EOF

host_arch="$(uname -m)"
xcrun clang "$check_dir/smoke.c" "$check_dir/libMacroNative-$host_arch.a" \
    -L "$swift_root/lib/swift/macosx" -L "$sdk/usr/lib/swift" \
    -lswiftCore -lswiftFoundation -lswiftDispatch \
    -framework AppKit -framework ScreenCaptureKit -framework CoreMedia \
    -framework CoreVideo -framework CoreGraphics -framework ApplicationServices \
    -framework Carbon -framework Foundation -o "$check_dir/smoke"
"$check_dir/smoke" "$native_dir/../../../fixtures/keyboard-monitor-routing.json"
printf '%s\n' 'Swift arm64/x86_64 compilation and host ABI smoke passed.'
