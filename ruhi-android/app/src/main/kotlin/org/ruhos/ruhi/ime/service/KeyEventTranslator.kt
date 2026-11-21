package org.ruhos.ruhi.ime.service

import android.text.InputType
import android.view.KeyCharacterMap
import android.view.KeyEvent

data class KeyInput(
  val keyCode: Int,
  val unicodeChar: Int,
  val isShiftPressed: Boolean = false,
  val isCtrlPressed: Boolean = false,
  val isAltPressed: Boolean = false,
  val isMetaPressed: Boolean = false,
  val isCapsLockOn: Boolean = false,
)

data class TranslatorState(
  val isReady: Boolean,
  val isPasswordOrRestricted: Boolean,
  val hasBuffer: Boolean,
  val keyboardMode: String,
  val fullwidthMode: Boolean,
)

data class TranslatorPrefs(
  val capsLockToggleMode: Boolean = true,
  val shiftSpaceAction: String = "passthrough",
  val altSpaceToggleMode: Boolean = true,
)

sealed class TranslatedKeyAction {
  data object PassThrough : TranslatedKeyAction()

  data object ToggleMode : TranslatedKeyAction()

  data object ToggleFullwidth : TranslatedKeyAction()

  data class SendKeyStr(val key: String) : TranslatedKeyAction()
}

object KeyEventTranslator {
  fun isPasswordOrRestrictedInputType(inputType: Int): Boolean {
    if (inputType == InputType.TYPE_NULL) return true
    val cls = inputType and InputType.TYPE_MASK_CLASS
    val variation = inputType and InputType.TYPE_MASK_VARIATION
    if (cls == InputType.TYPE_CLASS_NUMBER ||
      cls == InputType.TYPE_CLASS_PHONE ||
      cls == InputType.TYPE_CLASS_DATETIME
    ) {
      return true
    }
    if (cls == InputType.TYPE_CLASS_TEXT) {
      return variation == InputType.TYPE_TEXT_VARIATION_PASSWORD ||
        variation == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD ||
        variation == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD
    }
    return false
  }

  private fun isStandaloneOrSystemKey(keyCode: Int): Boolean {
    if (keyCode in KeyEvent.KEYCODE_F1..KeyEvent.KEYCODE_F12) return true
    return when (keyCode) {
      KeyEvent.KEYCODE_SHIFT_LEFT,
      KeyEvent.KEYCODE_SHIFT_RIGHT,
      KeyEvent.KEYCODE_CTRL_LEFT,
      KeyEvent.KEYCODE_CTRL_RIGHT,
      KeyEvent.KEYCODE_ALT_LEFT,
      KeyEvent.KEYCODE_ALT_RIGHT,
      KeyEvent.KEYCODE_META_LEFT,
      KeyEvent.KEYCODE_META_RIGHT,
      KeyEvent.KEYCODE_SYM,
      KeyEvent.KEYCODE_FUNCTION,
      KeyEvent.KEYCODE_NUM_LOCK,
      KeyEvent.KEYCODE_SCROLL_LOCK,
      KeyEvent.KEYCODE_BACK,
      KeyEvent.KEYCODE_HOME,
      KeyEvent.KEYCODE_APP_SWITCH,
      KeyEvent.KEYCODE_MENU,
      KeyEvent.KEYCODE_POWER,
      KeyEvent.KEYCODE_SLEEP,
      KeyEvent.KEYCODE_WAKEUP,
      KeyEvent.KEYCODE_VOLUME_UP,
      KeyEvent.KEYCODE_VOLUME_DOWN,
      KeyEvent.KEYCODE_VOLUME_MUTE,
      KeyEvent.KEYCODE_MEDIA_PLAY,
      KeyEvent.KEYCODE_MEDIA_PAUSE,
      KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE,
      KeyEvent.KEYCODE_MEDIA_NEXT,
      KeyEvent.KEYCODE_MEDIA_PREVIOUS,
      KeyEvent.KEYCODE_MEDIA_STOP,
      KeyEvent.KEYCODE_CAMERA,
      KeyEvent.KEYCODE_SEARCH,
      KeyEvent.KEYCODE_LANGUAGE_SWITCH -> true
      else -> false
    }
  }

  private fun mapCtrlCombo(keyCode: Int, isShift: Boolean): String? {
    val base =
      when (keyCode) {
        KeyEvent.KEYCODE_PERIOD -> "."
        KeyEvent.KEYCODE_COMMA -> ","
        KeyEvent.KEYCODE_MINUS -> "-"
        KeyEvent.KEYCODE_SEMICOLON -> ";"
        KeyEvent.KEYCODE_APOSTROPHE -> "'"
        KeyEvent.KEYCODE_SLASH -> "/"
        KeyEvent.KEYCODE_LEFT_BRACKET -> "["
        KeyEvent.KEYCODE_RIGHT_BRACKET -> "]"
        KeyEvent.KEYCODE_BACKSLASH -> "\\"
        KeyEvent.KEYCODE_GRAVE -> "`"
        KeyEvent.KEYCODE_1 -> "1"
        else -> null
      } ?: return null
    return if (isShift) "Ctrl+Shift+$base" else "Ctrl+$base"
  }

  private fun isWesternLeadingLetterOrSpecial(ch: Int): Boolean {
    if (ch == '`'.code || ch == '~'.code || ch == '\\'.code) return true
    if (ch in 'a'.code..'z'.code || ch in 'A'.code..'Z'.code) return true
    return (ch in 0x00C0..0x024F) || (ch in 0x1E00..0x1EFF)
  }

  fun translate(
    input: KeyInput,
    state: TranslatorState,
    prefs: TranslatorPrefs,
  ): TranslatedKeyAction {
    if (!state.isReady || state.isPasswordOrRestricted) {
      return TranslatedKeyAction.PassThrough
    }
    if (isStandaloneOrSystemKey(input.keyCode) || input.isMetaPressed) {
      return TranslatedKeyAction.PassThrough
    }

    if (input.keyCode == KeyEvent.KEYCODE_CAPS_LOCK) {
      return if (!input.isCtrlPressed && !input.isAltPressed && prefs.capsLockToggleMode) {
        TranslatedKeyAction.ToggleMode
      } else {
        TranslatedKeyAction.PassThrough
      }
    }

    if (input.keyCode == KeyEvent.KEYCODE_SPACE) {
      if (input.isAltPressed && !input.isCtrlPressed && !input.isShiftPressed && prefs.altSpaceToggleMode) {
        return TranslatedKeyAction.ToggleMode
      }
      if (input.isShiftPressed && !input.isCtrlPressed && !input.isAltPressed) {
        return when (prefs.shiftSpaceAction) {
          "toggle_mode" -> TranslatedKeyAction.ToggleMode
          "toggle_fullwidth" -> TranslatedKeyAction.ToggleFullwidth
          else -> TranslatedKeyAction.PassThrough
        }
      }
    }

    if (input.isAltPressed) {
      return TranslatedKeyAction.PassThrough
    }

    if (input.isCtrlPressed) {
      if (!state.hasBuffer) {
        return TranslatedKeyAction.PassThrough
      }
      return when (input.keyCode) {
        KeyEvent.KEYCODE_ENTER,
        KeyEvent.KEYCODE_NUMPAD_ENTER -> TranslatedKeyAction.SendKeyStr("Ctrl+Enter")
        KeyEvent.KEYCODE_DEL -> TranslatedKeyAction.SendKeyStr("Ctrl+BackSpace")
        KeyEvent.KEYCODE_DPAD_UP ->
          TranslatedKeyAction.SendKeyStr(if (input.isShiftPressed) "Ctrl+Shift+Up" else "Ctrl+Up")
        KeyEvent.KEYCODE_DPAD_DOWN ->
          TranslatedKeyAction.SendKeyStr(
            if (input.isShiftPressed) "Ctrl+Shift+Down" else "Ctrl+Down"
          )
        else -> {
          if (state.keyboardMode != "english") {
            mapCtrlCombo(input.keyCode, input.isShiftPressed)?.let {
              return TranslatedKeyAction.SendKeyStr(it)
            }
          }
          TranslatedKeyAction.PassThrough
        }
      }
    }

    when (input.keyCode) {
      KeyEvent.KEYCODE_ENTER,
      KeyEvent.KEYCODE_NUMPAD_ENTER ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr(if (input.isShiftPressed) "Shift+Enter" else "Enter")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_DEL ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr(
            if (input.isShiftPressed) "Shift+BackSpace" else "BackSpace"
          )
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_FORWARD_DEL -> return TranslatedKeyAction.PassThrough
      KeyEvent.KEYCODE_TAB ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr(if (input.isShiftPressed) "Shift+Tab" else "Tab")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_ESCAPE ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr("Escape")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_SPACE ->
        return if (state.hasBuffer || state.fullwidthMode) {
          TranslatedKeyAction.SendKeyStr("Space")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_DPAD_LEFT ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr(if (input.isShiftPressed) "Shift+Left" else "Left")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_DPAD_RIGHT ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr(if (input.isShiftPressed) "Shift+Right" else "Right")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_MOVE_HOME ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr(if (input.isShiftPressed) "Shift+Home" else "Home")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_MOVE_END ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr(if (input.isShiftPressed) "Shift+End" else "End")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_DPAD_UP ->
        return if (state.hasBuffer && !input.isShiftPressed) {
          TranslatedKeyAction.SendKeyStr("Up")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_DPAD_DOWN ->
        return if (state.hasBuffer && !input.isShiftPressed) {
          TranslatedKeyAction.SendKeyStr("Down")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_PAGE_UP ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr("PageUp")
        } else {
          TranslatedKeyAction.PassThrough
        }
      KeyEvent.KEYCODE_PAGE_DOWN ->
        return if (state.hasBuffer) {
          TranslatedKeyAction.SendKeyStr("PageDown")
        } else {
          TranslatedKeyAction.PassThrough
        }
    }

    // 許氏模式下，若硬體鍵盤曾遺留 CapsLock 狀態，非 Shift 按下字母一律強制為小寫注音鍵位
    if (state.keyboardMode != "english" &&
      input.keyCode in KeyEvent.KEYCODE_A..KeyEvent.KEYCODE_Z &&
      !input.isShiftPressed
    ) {
      val ch = ('a'.code + (input.keyCode - KeyEvent.KEYCODE_A)).toChar().toString()
      return TranslatedKeyAction.SendKeyStr(ch)
    }

    val uni = input.unicodeChar
    if (uni <= 0 || (uni and KeyCharacterMap.COMBINING_ACCENT) != 0 || uni < 0x20) {
      return TranslatedKeyAction.PassThrough
    }

    if (!state.hasBuffer && !state.fullwidthMode) {
      if (state.keyboardMode == "english") {
        if (!isWesternLeadingLetterOrSpecial(uni)) {
          return TranslatedKeyAction.PassThrough
        }
      } else {
        // 許氏半形無組字時，數字鍵直通應用程式；標點與字母由引擎接管
        if (uni in '0'.code..'9'.code) {
          return TranslatedKeyAction.PassThrough
        }
      }
    }

    val text = String(Character.toChars(uni))
    return TranslatedKeyAction.SendKeyStr(text)
  }
}
