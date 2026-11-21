package org.ruhos.ruhi.ime.nativebridge

object RuhiNativeBridge {
  init {
    System.loadLibrary("ruhi_android_jni")
  }

  external fun nativeCreate(dataDir: String, configJson: String): Long

  external fun nativeDestroy(handle: Long)

  external fun nativeApplyConfig(handle: Long, configJson: String): String?

  external fun nativeProcessKeyStr(handle: Long, keyStr: String): String?

  external fun nativeToggleMode(handle: Long): String?

  external fun nativeToggleFullwidth(handle: Long): String?

  external fun nativeSelectCandidate(handle: Long, index: Int): String?

  external fun nativeCommitAll(handle: Long): String?

  external fun nativeReset(handle: Long): String?

  external fun nativeGetState(handle: Long): String?
}
