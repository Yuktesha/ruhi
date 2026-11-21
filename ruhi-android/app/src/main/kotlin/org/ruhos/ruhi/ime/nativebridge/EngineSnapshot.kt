package org.ruhos.ruhi.ime.nativebridge

import org.json.JSONObject

data class EngineSnapshot(
  val action: String,
  val uiMode: String,
  val keyboardMode: String,
  val modeBadge: String,
  val fullwidthMode: Boolean,
  val hasBuffer: Boolean,
  val showCandidates: Boolean,
  val quickSelectActive: Boolean,
  val quickKeyMode: String,
  val preedit: String,
  val composed: String,
  val cursorPos: Int,
  val selectionStart: Int,
  val selectionEnd: Int,
  val candidates: List<String>,
  val candidateIdx: Int,
  val pageSize: Int,
  val committedChunk: String,
) {
  companion object {
    fun fromJson(raw: String?): EngineSnapshot? {
      if (raw.isNullOrBlank()) return null
      return try {
        val obj = JSONObject(raw)
        val candsArr = obj.optJSONArray("candidates")
        val cands =
          if (candsArr != null) {
            List(candsArr.length()) { i -> candsArr.optString(i, "") }
          } else {
            emptyList()
          }
        EngineSnapshot(
          action = obj.optString("action", "none"),
          uiMode = obj.optString("ui_mode", "normal"),
          keyboardMode = obj.optString("keyboard_mode", "hsu"),
          modeBadge = obj.optString("mode_badge", "許"),
          fullwidthMode = obj.optBoolean("fullwidth_mode", false),
          hasBuffer = obj.optBoolean("has_buffer", false),
          showCandidates = obj.optBoolean("show_candidates", false),
          quickSelectActive = obj.optBoolean("quick_select_active", false),
          quickKeyMode = obj.optString("quick_key_mode", "asdf"),
          preedit = obj.optString("preedit", ""),
          composed = obj.optString("composed", ""),
          cursorPos = obj.optInt("cursor_pos", 0),
          selectionStart = obj.optInt("selection_start", -1),
          selectionEnd = obj.optInt("selection_end", -1),
          candidates = cands,
          candidateIdx = obj.optInt("candidate_idx", 0),
          pageSize = obj.optInt("page_size", 9).coerceAtLeast(1),
          committedChunk = obj.optString("committed_chunk", ""),
        )
      } catch (_: Exception) {
        null
      }
    }
  }
}
