package org.ruhos.ruhi.ime.service

import android.text.InputType
import android.view.KeyCharacterMap
import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class KeyEventTranslatorTest {
  private val defaultPrefs = TranslatorPrefs(
    capsLockToggleMode = true,
    shiftSpaceAction = "passthrough",
    altSpaceToggleMode = true,
  )

  private fun idleHsuState() = TranslatorState(
    isReady = true,
    isPasswordOrRestricted = false,
    hasBuffer = false,
    keyboardMode = "hsu",
    fullwidthMode = false,
  )

  private fun activeHsuState() = idleHsuState().copy(hasBuffer = true)

  private fun idleEnglishState() = idleHsuState().copy(keyboardMode = "english")

  private fun activeEnglishState() = idleEnglishState().copy(hasBuffer = true)

  @Test
  fun passwordAndRestrictedFieldsAlwaysPassThrough() {
    assertTrue(
      KeyEventTranslator.isPasswordOrRestrictedInputType(
        InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
      )
    )
    assertTrue(
      KeyEventTranslator.isPasswordOrRestrictedInputType(
        InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD
      )
    )
    assertTrue(KeyEventTranslator.isPasswordOrRestrictedInputType(InputType.TYPE_CLASS_NUMBER))
    assertFalse(KeyEventTranslator.isPasswordOrRestrictedInputType(InputType.TYPE_CLASS_TEXT))

    val action =
      KeyEventTranslator.translate(
        KeyInput(KeyEvent.KEYCODE_A, 'a'.code),
        idleHsuState().copy(isPasswordOrRestricted = true),
        defaultPrefs,
      )
    assertEquals(TranslatedKeyAction.PassThrough, action)
  }

  @Test
  fun shiftSpaceYieldsToSystemByDefaultAndSupportsConfigurableActions() {
    val shiftSpace = KeyInput(KeyEvent.KEYCODE_SPACE, ' '.code, isShiftPressed = true)
    assertEquals(
      TranslatedKeyAction.PassThrough,
      KeyEventTranslator.translate(shiftSpace, idleHsuState(), defaultPrefs),
    )
    assertEquals(
      TranslatedKeyAction.ToggleMode,
      KeyEventTranslator.translate(
        shiftSpace,
        idleHsuState(),
        defaultPrefs.copy(shiftSpaceAction = "toggle_mode"),
      ),
    )
    assertEquals(
      TranslatedKeyAction.ToggleFullwidth,
      KeyEventTranslator.translate(
        shiftSpace,
        idleHsuState(),
        defaultPrefs.copy(shiftSpaceAction = "toggle_fullwidth"),
      ),
    )
  }

  @Test
  fun capsLockAndAltSpaceToggleMode() {
    assertEquals(
      TranslatedKeyAction.ToggleMode,
      KeyEventTranslator.translate(
        KeyInput(KeyEvent.KEYCODE_CAPS_LOCK, 0),
        idleHsuState(),
        defaultPrefs,
      ),
    )
    assertEquals(
      TranslatedKeyAction.ToggleMode,
      KeyEventTranslator.translate(
        KeyInput(KeyEvent.KEYCODE_SPACE, ' '.code, isAltPressed = true),
        idleEnglishState(),
        defaultPrefs,
      ),
    )
  }

  @Test
  fun ctrlShortcutsPassThroughWhenBufferEmpty() {
    val ctrlV = KeyInput(KeyEvent.KEYCODE_V, 'v'.code, isCtrlPressed = true)
    val ctrlC = KeyInput(KeyEvent.KEYCODE_C, 'c'.code, isCtrlPressed = true)
    assertEquals(
      TranslatedKeyAction.PassThrough,
      KeyEventTranslator.translate(ctrlV, idleHsuState(), defaultPrefs),
    )
    assertEquals(
      TranslatedKeyAction.PassThrough,
      KeyEventTranslator.translate(ctrlC, idleEnglishState(), defaultPrefs),
    )
  }

  @Test
  fun navigationAndEscapePassThroughWhenIdleAndInterceptedWhenComposing() {
    val esc = KeyInput(KeyEvent.KEYCODE_ESCAPE, 0)
    val enter = KeyInput(KeyEvent.KEYCODE_ENTER, '\n'.code)
    val shiftLeft = KeyInput(KeyEvent.KEYCODE_DPAD_LEFT, 0, isShiftPressed = true)

    assertEquals(
      TranslatedKeyAction.PassThrough,
      KeyEventTranslator.translate(esc, idleHsuState(), defaultPrefs),
    )
    assertEquals(
      TranslatedKeyAction.PassThrough,
      KeyEventTranslator.translate(enter, idleHsuState(), defaultPrefs),
    )

    assertEquals(
      TranslatedKeyAction.SendKeyStr("Escape"),
      KeyEventTranslator.translate(esc, activeHsuState(), defaultPrefs),
    )
    assertEquals(
      TranslatedKeyAction.SendKeyStr("Enter"),
      KeyEventTranslator.translate(enter, activeHsuState(), defaultPrefs),
    )
    assertEquals(
      TranslatedKeyAction.SendKeyStr("Shift+Left"),
      KeyEventTranslator.translate(shiftLeft, activeHsuState(), defaultPrefs),
    )
  }

  @Test
  fun hsuIgnoresHardwareCapsLockAndPassesThroughIdleDigits() {
    val capsLedA = KeyInput(KeyEvent.KEYCODE_A, 'A'.code, isShiftPressed = false, isCapsLockOn = true)
    assertEquals(
      TranslatedKeyAction.SendKeyStr("a"),
      KeyEventTranslator.translate(capsLedA, idleHsuState(), defaultPrefs),
    )
    val idleDigit1 = KeyInput(KeyEvent.KEYCODE_1, '1'.code)
    assertEquals(
      TranslatedKeyAction.PassThrough,
      KeyEventTranslator.translate(idleDigit1, idleHsuState(), defaultPrefs),
    )
    assertEquals(
      TranslatedKeyAction.SendKeyStr("1"),
      KeyEventTranslator.translate(idleDigit1, activeHsuState(), defaultPrefs),
    )
  }

  @Test
  fun englishModePreservesUppercasePassesIdleSlashAndDeadKeys() {
    val shiftA = KeyInput(KeyEvent.KEYCODE_A, 'A'.code, isShiftPressed = true)
    assertEquals(
      TranslatedKeyAction.SendKeyStr("A"),
      KeyEventTranslator.translate(shiftA, idleEnglishState(), defaultPrefs),
    )
    val idleSlash = KeyInput(KeyEvent.KEYCODE_SLASH, '/'.code)
    assertEquals(
      TranslatedKeyAction.PassThrough,
      KeyEventTranslator.translate(idleSlash, idleEnglishState(), defaultPrefs),
    )
    val deadAccent = KeyInput(KeyEvent.KEYCODE_E, KeyCharacterMap.COMBINING_ACCENT or '`'.code)
    assertEquals(
      TranslatedKeyAction.PassThrough,
      KeyEventTranslator.translate(deadAccent, activeEnglishState(), defaultPrefs),
    )
  }
}
