#include <android/log.h>
#include <jni.h>

#include <string>
#include <vector>

#include "whisper.h"

namespace {

constexpr const char *kTag = "bridgething-whisper";

void forward_log(ggml_log_level level, const char *text, void *) {
  int priority = ANDROID_LOG_DEBUG;
  switch (level) {
    case GGML_LOG_LEVEL_ERROR:
      priority = ANDROID_LOG_ERROR;
      break;
    case GGML_LOG_LEVEL_WARN:
      priority = ANDROID_LOG_WARN;
      break;
    case GGML_LOG_LEVEL_INFO:
      priority = ANDROID_LOG_INFO;
      break;
    default:
      break;
  }
  __android_log_write(priority, kTag, text);
}

whisper_context *as_context(jlong handle) { return reinterpret_cast<whisper_context *>(handle); }

std::string to_utf8(JNIEnv *env, jstring value) {
  const char *chars = env->GetStringUTFChars(value, nullptr);
  std::string out(chars == nullptr ? "" : chars);
  if (chars != nullptr) {
    env->ReleaseStringUTFChars(value, chars);
  }
  return out;
}

}  // namespace

extern "C" {

JNIEXPORT jlong JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeInit(
    JNIEnv *env, jobject, jstring model_path) {
  whisper_log_set(forward_log, nullptr);

  const std::string path = to_utf8(env, model_path);

  whisper_context_params cparams = whisper_context_default_params();
  cparams.use_gpu = false;
  cparams.flash_attn = false;

  whisper_context *ctx = whisper_init_from_file_with_params(path.c_str(), cparams);
  return reinterpret_cast<jlong>(ctx);
}

JNIEXPORT void JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeRelease(
    JNIEnv *, jobject, jlong handle) {
  if (handle != 0) {
    whisper_free(as_context(handle));
  }
}

JNIEXPORT jint JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeFull(
    JNIEnv *env, jobject, jlong handle, jfloatArray samples, jint threads, jstring language) {
  whisper_context *ctx = as_context(handle);
  if (ctx == nullptr) {
    return -1;
  }

  const jsize count = env->GetArrayLength(samples);
  std::vector<float> pcm(static_cast<size_t>(count));
  env->GetFloatArrayRegion(samples, 0, count, pcm.data());

  const std::string lang = to_utf8(env, language);

  whisper_full_params params = whisper_full_default_params(WHISPER_SAMPLING_GREEDY);
  params.n_threads = threads;
  params.language = lang.c_str();
  params.detect_language = false;
  params.translate = false;
  params.no_context = true;
  params.print_progress = false;
  params.print_realtime = false;
  params.print_timestamps = false;
  params.print_special = false;
  params.temperature = 0.0f;
  params.greedy.best_of = 1;

  return whisper_full(ctx, params, pcm.data(), count);
}

JNIEXPORT jint JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeSegmentCount(
    JNIEnv *, jobject, jlong handle) {
  whisper_context *ctx = as_context(handle);
  return ctx == nullptr ? 0 : whisper_full_n_segments(ctx);
}

JNIEXPORT jstring JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeSegmentText(
    JNIEnv *env, jobject, jlong handle, jint index) {
  whisper_context *ctx = as_context(handle);
  const char *text = ctx == nullptr ? "" : whisper_full_get_segment_text(ctx, index);
  return env->NewStringUTF(text == nullptr ? "" : text);
}

JNIEXPORT jlong JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeSegmentStartMs(
    JNIEnv *, jobject, jlong handle, jint index) {
  whisper_context *ctx = as_context(handle);
  // whisper timestamps are centiseconds.
  return ctx == nullptr ? 0 : whisper_full_get_segment_t0(ctx, index) * 10;
}

JNIEXPORT jlong JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeSegmentEndMs(
    JNIEnv *, jobject, jlong handle, jint index) {
  whisper_context *ctx = as_context(handle);
  return ctx == nullptr ? 0 : whisper_full_get_segment_t1(ctx, index) * 10;
}

JNIEXPORT jfloat JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeSegmentConfidence(
    JNIEnv *, jobject, jlong handle, jint index) {
  whisper_context *ctx = as_context(handle);
  if (ctx == nullptr) {
    return 0.0f;
  }

  const int tokens = whisper_full_n_tokens(ctx, index);
  if (tokens <= 0) {
    return 0.0f;
  }

  float total = 0.0f;
  for (int i = 0; i < tokens; ++i) {
    total += whisper_full_get_token_p(ctx, index, i);
  }
  return total / static_cast<float>(tokens);
}

JNIEXPORT jstring JNICALL Java_com_bridgething_asr_whisper_NativeWhisperBinding_nativeSystemInfo(
    JNIEnv *env, jobject) {
  std::string info = std::string(whisper_version()) + " " + whisper_print_system_info();
  return env->NewStringUTF(info.c_str());
}

}  // extern "C"
