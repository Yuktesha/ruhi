package org.ruhos.ruhi.ime.config

import android.content.Context
import android.content.SharedPreferences
import org.json.JSONArray
import org.json.JSONObject

data class RuhiImeSettings(
  val westernLanguages: Set<String> = setOf("en", "sa_am"),
  val capsLockToggleMode: Boolean = true,
  val shiftSpaceAction: String = SHIFT_SPACE_PASSTHROUGH,
  val altSpaceToggleMode: Boolean = true,
  val halfwidthPunctMode: String = "fullwidth",
  val candidateQuickKey: String = "auto",
  val tabKeyMode: String = "associative",
  val tildeToggleMode: String = "punct",
  val preferredInitialMode: String = "hsu",
  val configVersion: Long = 1L,
) {
  fun toEngineConfigJson(initialModeOverride: String? = null): String {
    val orderedLangs = listOf("en", "sa_am", "fr", "de", "es").filter { westernLanguages.contains(it) }
    val langsJson = JSONArray()
    if (orderedLangs.isEmpty()) {
      langsJson.put("en")
    } else {
      orderedLangs.forEach { langsJson.put(it) }
    }

    return JSONObject()
      .put("western_languages", langsJson)
      .put("halfwidth_punct_mode", halfwidthPunctMode)
      .put("candidate_quick_key", candidateQuickKey)
      .put("tab_key_mode", tabKeyMode)
      .put("tilde_toggle_mode", tildeToggleMode)
      .put("shift_space_toggle_fullwidth", shiftSpaceAction == SHIFT_SPACE_TOGGLE_FULLWIDTH)
      .put("enable_mixed_english", false)
      .put("initial_keyboard_mode", initialModeOverride ?: preferredInitialMode)
      .toString()
  }

  companion object {
    const val SHIFT_SPACE_PASSTHROUGH = "passthrough"
    const val SHIFT_SPACE_TOGGLE_MODE = "toggle_mode"
    const val SHIFT_SPACE_TOGGLE_FULLWIDTH = "toggle_fullwidth"
  }
}

object RuhiPrefs {
  private const val PREF_NAME = "ruhi_ime_prefs"
  private const val KEY_WESTERN_LANGS = "western_languages"
  private const val KEY_CAPSLOCK_TOGGLE = "caps_lock_toggle_mode"
  private const val KEY_SHIFT_SPACE_ACTION = "shift_space_action"
  private const val KEY_ALT_SPACE_TOGGLE = "alt_space_toggle_mode"
  private const val KEY_HALFWIDTH_PUNCT = "halfwidth_punct_mode"
  private const val KEY_QUICK_KEY = "candidate_quick_key"
  private const val KEY_TAB_KEY_MODE = "tab_key_mode"
  private const val KEY_TILDE_MODE = "tilde_toggle_mode"
  private const val KEY_INITIAL_MODE = "preferred_initial_mode"
  private const val KEY_VERSION = "config_version"

  fun prefs(context: Context): SharedPreferences =
    context.getSharedPreferences(PREF_NAME, Context.MODE_PRIVATE)

  fun load(context: Context): RuhiImeSettings {
    val sp = prefs(context)
    val langs = sp.getStringSet(KEY_WESTERN_LANGS, null)?.toSet() ?: setOf("en", "sa_am")
    return RuhiImeSettings(
      westernLanguages = if (langs.isEmpty()) setOf("en") else langs,
      capsLockToggleMode = sp.getBoolean(KEY_CAPSLOCK_TOGGLE, true),
      shiftSpaceAction =
        sp.getString(KEY_SHIFT_SPACE_ACTION, RuhiImeSettings.SHIFT_SPACE_PASSTHROUGH)
          ?: RuhiImeSettings.SHIFT_SPACE_PASSTHROUGH,
      altSpaceToggleMode = sp.getBoolean(KEY_ALT_SPACE_TOGGLE, true),
      halfwidthPunctMode = sp.getString(KEY_HALFWIDTH_PUNCT, "fullwidth") ?: "fullwidth",
      candidateQuickKey = sp.getString(KEY_QUICK_KEY, "auto") ?: "auto",
      tabKeyMode = sp.getString(KEY_TAB_KEY_MODE, "associative") ?: "associative",
      tildeToggleMode = sp.getString(KEY_TILDE_MODE, "punct") ?: "punct",
      preferredInitialMode = sp.getString(KEY_INITIAL_MODE, "hsu") ?: "hsu",
      configVersion = sp.getLong(KEY_VERSION, 1L),
    )
  }

  fun update(context: Context, transform: (RuhiImeSettings) -> RuhiImeSettings): RuhiImeSettings {
    val cur = load(context)
    val next = transform(cur).copy(configVersion = cur.configVersion + 1L)
    prefs(context)
      .edit()
      .putStringSet(KEY_WESTERN_LANGS, next.westernLanguages)
      .putBoolean(KEY_CAPSLOCK_TOGGLE, next.capsLockToggleMode)
      .putString(KEY_SHIFT_SPACE_ACTION, next.shiftSpaceAction)
      .putBoolean(KEY_ALT_SPACE_TOGGLE, next.altSpaceToggleMode)
      .putString(KEY_HALFWIDTH_PUNCT, next.halfwidthPunctMode)
      .putString(KEY_QUICK_KEY, next.candidateQuickKey)
      .putString(KEY_TAB_KEY_MODE, next.tabKeyMode)
      .putString(KEY_TILDE_MODE, next.tildeToggleMode)
      .putString(KEY_INITIAL_MODE, next.preferredInitialMode)
      .putLong(KEY_VERSION, next.configVersion)
      .apply()
    return next
  }

  fun saveLastKeyboardMode(context: Context, mode: String) {
    val norm = if (mode == "english") "english" else "hsu"
    prefs(context).edit().putString(KEY_INITIAL_MODE, norm).apply()
  }
}
