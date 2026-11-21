package org.ruhos.ruhi.ime.service

import android.content.Intent
import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.view.KeyEvent
import android.view.View
import android.view.inputmethod.EditorInfo
import org.ruhos.ruhi.ime.MainActivity
import org.ruhos.ruhi.ime.config.RuhiImeSettings
import org.ruhos.ruhi.ime.config.RuhiPrefs
import org.ruhos.ruhi.ime.data.RuhiAssetManager
import org.ruhos.ruhi.ime.nativebridge.EngineSnapshot
import org.ruhos.ruhi.ime.nativebridge.RuhiNativeBridge
import org.ruhos.ruhi.ime.ui.InputDeskView
import java.util.Collections
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

class RuhiInputMethodService : InputMethodService() {
  private val mainHandler = Handler(Looper.getMainLooper())
  private val engineExecutor =
    Executors.newSingleThreadExecutor { runnable ->
      Thread(runnable, "ruhi-engine-worker").apply { isDaemon = true }
    }

  @Volatile private var engineHandle: Long = 0L

  @Volatile private var engineReady: Boolean = false

  @Volatile private var latestHasBuffer: Boolean = false

  @Volatile private var latestKeyboardMode: String = "hsu"

  @Volatile private var latestFullwidthMode: Boolean = false

  @Volatile private var isPasswordField: Boolean = false

  @Volatile private var latestSnapshot: EngineSnapshot? = null

  @Volatile private var appliedConfigVersion: Long = 0L

  @Volatile private var currentSettings: RuhiImeSettings = RuhiImeSettings()

  private val consumedKeyCodes: MutableSet<Int> = Collections.synchronizedSet(HashSet())

  private var deskView: InputDeskView? = null

  override fun onCreate() {
    super.onCreate()
    val settings = RuhiPrefs.load(this)
    currentSettings = settings
    appliedConfigVersion = settings.configVersion
    latestKeyboardMode = settings.preferredInitialMode

    engineExecutor.execute {
      try {
        val dataDir = RuhiAssetManager.ensureExtracted(applicationContext)
        val handle =
          RuhiNativeBridge.nativeCreate(
            dataDir.absolutePath,
            settings.toEngineConfigJson(settings.preferredInitialMode),
          )
        if (handle != 0L) {
          engineHandle = handle
          val snap = EngineSnapshot.fromJson(RuhiNativeBridge.nativeGetState(handle))
          engineReady = true
          if (snap != null) {
            updateMirrors(snap)
          }
          mainHandler.post { refreshDeskView() }
        }
      } catch (_: Throwable) {
        engineReady = false
      }
    }
  }

  override fun onCreateInputView(): View {
    val view =
      InputDeskView(
        context = this,
        onToggleMode = { dispatchEngineOp { RuhiNativeBridge.nativeToggleMode(it) } },
        onToggleFullwidth = { dispatchEngineOp { RuhiNativeBridge.nativeToggleFullwidth(it) } },
        onCandidateClick = { idx ->
          dispatchEngineOp { RuhiNativeBridge.nativeSelectCandidate(it, idx) }
        },
        onPagePrev = { dispatchEngineOp { RuhiNativeBridge.nativeProcessKeyStr(it, "PageUp") } },
        onPageNext = { dispatchEngineOp { RuhiNativeBridge.nativeProcessKeyStr(it, "PageDown") } },
        onCommit = { dispatchEngineOp { RuhiNativeBridge.nativeCommitAll(it) } },
        onReset = { dispatchEngineOp { RuhiNativeBridge.nativeReset(it) } },
        onOpenSettings = {
          val intent =
            Intent(this, MainActivity::class.java).apply {
              addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
          startActivity(intent)
        },
      )
    deskView = view
    refreshDeskView()
    return view
  }

  override fun onEvaluateInputViewShown(): Boolean {
    // 確保 Galaxy Z Flip 5 One UI 外接實體鍵盤時仍可顯示底部輸入座與候選字列
    return true
  }

  override fun onEvaluateFullscreenMode(): Boolean {
    // 永不進入全螢幕編輯模式，維持 90° Flex Mode 與一般視窗正常輸入體驗
    return false
  }

  override fun onStartInput(attribute: EditorInfo?, restarting: Boolean) {
    super.onStartInput(attribute, restarting)
    consumedKeyCodes.clear()
    isPasswordField =
      attribute != null && KeyEventTranslator.isPasswordOrRestrictedInputType(attribute.inputType)

    val latestCfg = RuhiPrefs.load(this)
    currentSettings = latestCfg
    if (engineReady && engineHandle != 0L) {
      if (latestHasBuffer && !restarting) {
        runEngineSync { RuhiNativeBridge.nativeReset(it) }
      }
      if (latestCfg.configVersion != appliedConfigVersion) {
        appliedConfigVersion = latestCfg.configVersion
        val cfgJson = latestCfg.toEngineConfigJson(latestKeyboardMode)
        runEngineSync { RuhiNativeBridge.nativeApplyConfig(it, cfgJson) }
      }
    }
    mainHandler.post { refreshDeskView() }
  }

  override fun onFinishInput() {
    super.onFinishInput()
    consumedKeyCodes.clear()
    if (engineReady && engineHandle != 0L && latestHasBuffer) {
      val snap = runEngineSync { RuhiNativeBridge.nativeCommitAll(it) }
      if (snap != null && snap.committedChunk.isNotEmpty()) {
        currentInputConnection?.commitText(snap.committedChunk, 1)
      }
      mainHandler.post { refreshDeskView() }
    }
  }

  override fun onKeyDown(keyCode: Int, event: KeyEvent): Boolean {
    if (!engineReady || engineHandle == 0L) {
      return super.onKeyDown(keyCode, event)
    }

    val latestCfg = RuhiPrefs.load(this)
    currentSettings = latestCfg
    if (latestCfg.configVersion != appliedConfigVersion) {
      appliedConfigVersion = latestCfg.configVersion
      val cfgJson = latestCfg.toEngineConfigJson(latestKeyboardMode)
      engineExecutor.execute {
        val h = engineHandle
        if (h != 0L) {
          RuhiNativeBridge.nativeApplyConfig(h, cfgJson)
        }
      }
    }

    val metaForUnicode =
      if (latestCfg.capsLockToggleMode) {
        event.metaState and KeyEvent.META_CAPS_LOCK_ON.inv()
      } else {
        event.metaState
      }
    val keyInput =
      KeyInput(
        keyCode = keyCode,
        unicodeChar = event.getUnicodeChar(metaForUnicode),
        isShiftPressed = event.isShiftPressed,
        isCtrlPressed = event.isCtrlPressed,
        isAltPressed = event.isAltPressed,
        isMetaPressed = event.isMetaPressed,
        isCapsLockOn = event.isCapsLockOn,
      )
    val state =
      TranslatorState(
        isReady = engineReady,
        isPasswordOrRestricted = isPasswordField,
        hasBuffer = latestHasBuffer,
        keyboardMode = latestKeyboardMode,
        fullwidthMode = latestFullwidthMode,
      )
    val prefs =
      TranslatorPrefs(
        capsLockToggleMode = latestCfg.capsLockToggleMode,
        shiftSpaceAction = latestCfg.shiftSpaceAction,
        altSpaceToggleMode = latestCfg.altSpaceToggleMode,
      )

    val action = KeyEventTranslator.translate(keyInput, state, prefs)
    if (action == TranslatedKeyAction.PassThrough) {
      return super.onKeyDown(keyCode, event)
    }

    consumedKeyCodes.add(keyCode)
    val snap =
      when (action) {
        is TranslatedKeyAction.ToggleMode -> {
          val s = runEngineSync { RuhiNativeBridge.nativeToggleMode(it) }
          if (s != null) {
            RuhiPrefs.saveLastKeyboardMode(this, s.keyboardMode)
          }
          s
        }
        is TranslatedKeyAction.ToggleFullwidth ->
          runEngineSync { RuhiNativeBridge.nativeToggleFullwidth(it) }
        is TranslatedKeyAction.SendKeyStr ->
          runEngineSync { RuhiNativeBridge.nativeProcessKeyStr(it, action.key) }
        TranslatedKeyAction.PassThrough -> null
      }

    if (snap != null) {
      applySnapshotToUi(snap)
    }
    return true
  }

  override fun onKeyUp(keyCode: Int, event: KeyEvent): Boolean {
    if (consumedKeyCodes.remove(keyCode)) {
      return true
    }
    return super.onKeyUp(keyCode, event)
  }

  override fun onDestroy() {
    super.onDestroy()
    val handle = engineHandle
    engineHandle = 0L
    engineReady = false
    engineExecutor.execute {
      if (handle != 0L) {
        RuhiNativeBridge.nativeDestroy(handle)
      }
    }
    engineExecutor.shutdown()
  }

  private fun runEngineSync(op: (Long) -> String?): EngineSnapshot? {
    val future =
      engineExecutor.submit<EngineSnapshot?> {
        val h = engineHandle
        if (h == 0L) return@submit null
        val json = op(h)
        val snap = EngineSnapshot.fromJson(json)
        if (snap != null) {
          updateMirrors(snap)
        }
        snap
      }
    return try {
      future.get(150, TimeUnit.MILLISECONDS)
    } catch (_: Exception) {
      null
    }
  }

  private fun dispatchEngineOp(op: (Long) -> String?) {
    if (!engineReady || engineHandle == 0L) return
    engineExecutor.execute {
      val h = engineHandle
      if (h == 0L) return@execute
      val snap = EngineSnapshot.fromJson(op(h)) ?: return@execute
      updateMirrors(snap)
      if (snap.action == "switch_keyboard_mode") {
        RuhiPrefs.saveLastKeyboardMode(applicationContext, snap.keyboardMode)
      }
      mainHandler.post { applySnapshotToUi(snap) }
    }
  }

  private fun updateMirrors(snap: EngineSnapshot) {
    latestSnapshot = snap
    latestHasBuffer = snap.hasBuffer
    latestKeyboardMode = snap.keyboardMode
    latestFullwidthMode = snap.fullwidthMode
  }

  private fun applySnapshotToUi(snap: EngineSnapshot) {
    if (snap.committedChunk.isNotEmpty()) {
      currentInputConnection?.commitText(snap.committedChunk, 1)
    }
    deskView?.render(snap, engineReady, isPasswordField)
  }

  private fun refreshDeskView() {
    deskView?.render(latestSnapshot, engineReady, isPasswordField)
  }
}
