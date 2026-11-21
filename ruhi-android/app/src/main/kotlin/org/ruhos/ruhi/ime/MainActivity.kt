package org.ruhos.ruhi.ime

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import org.ruhos.ruhi.ime.config.RuhiImeSettings
import org.ruhos.ruhi.ime.config.RuhiPrefs
import org.ruhos.ruhi.ime.data.RuhiAssetManager
import org.ruhos.ruhi.ime.ui.theme.Theme

class MainActivity : ComponentActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    enableEdgeToEdge()
    setContent {
      Theme {
        Surface(
          modifier = Modifier.fillMaxSize(),
          color = MaterialTheme.colorScheme.background,
        ) {
          RuhiSettingsScreen()
        }
      }
    }
  }
}

private fun isRuhiImeEnabled(context: Context): Boolean {
  val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
  return imm.enabledInputMethodList.any { it.packageName == context.packageName }
}

private fun isRuhiImeSelected(context: Context): Boolean {
  val cur = Settings.Secure.getString(context.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
  return cur?.startsWith("${context.packageName}/") == true
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun RuhiSettingsScreen() {
  val context = LocalContext.current
  val lifecycleOwner = LocalLifecycleOwner.current

  var enabled by remember { mutableStateOf(isRuhiImeEnabled(context)) }
  var selected by remember { mutableStateOf(isRuhiImeSelected(context)) }
  var settings by remember { mutableStateOf(RuhiPrefs.load(context)) }
  var testText by remember { mutableStateOf("") }
  var passwordTest by remember { mutableStateOf("") }
  var userDictText by remember { mutableStateOf(RuhiAssetManager.readUserDict(context)) }

  DisposableEffect(lifecycleOwner) {
    val obs = LifecycleEventObserver { _, event ->
      if (event == Lifecycle.Event.ON_RESUME) {
        enabled = isRuhiImeEnabled(context)
        selected = isRuhiImeSelected(context)
        settings = RuhiPrefs.load(context)
      }
    }
    lifecycleOwner.lifecycle.addObserver(obs)
    onDispose { lifecycleOwner.lifecycle.removeObserver(obs) }
  }

  fun updateSettings(transform: (RuhiImeSettings) -> RuhiImeSettings) {
    settings = RuhiPrefs.update(context, transform)
  }

  Column(
    modifier =
      Modifier.fillMaxSize()
        .safeDrawingPadding()
        .verticalScroll(rememberScrollState())
        .padding(horizontal = 16.dp, vertical = 12.dp),
    verticalArrangement = Arrangement.spacedBy(12.dp),
  ) {
    Text(
      text = "如喜（RuhI）通用智慧型輸入法",
      style = MaterialTheme.typography.headlineSmall,
      fontWeight = FontWeight.Bold,
    )
    Text(
      text = "Android 外接實體鍵盤版 · 許氏雙拼 × 歐文鍵盤（純 Rust 核心）",
      style = MaterialTheme.typography.bodyMedium,
      color = MaterialTheme.colorScheme.onSurfaceVariant,
    )

    // 1. 啟用與切換狀態卡
    Card(modifier = Modifier.fillMaxWidth()) {
      Column(
        modifier = Modifier.padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
      ) {
        Text(
          text = "步驟一：啟用與切換輸入法",
          style = MaterialTheme.typography.titleMedium,
          fontWeight = FontWeight.SemiBold,
        )
        Text(
          text =
            "目前狀態：" +
              when {
                selected -> "已啟用並設為目前輸入法"
                enabled -> "已啟用，請切換至如喜輸入法"
                else -> "尚未在系統中啟用"
              },
          style = MaterialTheme.typography.bodyMedium,
          color =
            if (selected) MaterialTheme.colorScheme.primary
            else MaterialTheme.colorScheme.onSurfaceVariant,
        )
        FlowRow(
          horizontalArrangement = Arrangement.spacedBy(8.dp),
          verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
          Button(
            onClick = {
              context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
            }
          ) {
            Text(if (enabled) "管理系統鍵盤" else "前往啟用如喜")
          }
          OutlinedButton(
            onClick = {
              val imm =
                context.getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
              imm.showInputMethodPicker()
            }
          ) {
            Text("選擇目前輸入法")
          }
        }
      }
    }

    // 2. 外接鍵盤速查與實機測試區
    Card(modifier = Modifier.fillMaxWidth()) {
      Column(
        modifier = Modifier.padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
      ) {
        Text(
          text = "步驟二：外接鍵盤操作速查與實打測試",
          style = MaterialTheme.typography.titleMedium,
          fontWeight = FontWeight.SemiBold,
        )
        Text(
          text =
            "• 中／歐切換：單按 Caps Lock（或備用 Alt + Space／點選底部 [許][歐]）\n" +
              "• 全／半形切換：點選底部 [半]／[全] 徽章（Shift + Space 預設禮讓系統）\n" +
              "• 許氏選字：按 ↓ 開啟候選列後，用 asdfghjkl 或 1–9 快選，Esc 僅關候選列\n" +
              "• 歐文智慧：空白鍵自動勘誤（按一次退格還原）、Tab 語境補全、↑ 音韻救援、↓ 變音／梵語",
          style = MaterialTheme.typography.bodySmall,
          color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedTextField(
          value = testText,
          onValueChange = { testText = it },
          modifier = Modifier.fillMaxWidth(),
          label = { Text("實機盲打測試欄（請接上外接鍵盤試打）") },
          minLines = 3,
          maxLines = 5,
        )
        OutlinedTextField(
          value = passwordTest,
          onValueChange = { passwordTest = it },
          modifier = Modifier.fillMaxWidth(),
          label = { Text("密碼欄直通測試（應自動放行，不攔截組字）") },
          visualTransformation = PasswordVisualTransformation(),
          keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
          singleLine = true,
        )
      }
    }

    // 3. 偏好設定區
    Card(modifier = Modifier.fillMaxWidth()) {
      Column(
        modifier = Modifier.padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
      ) {
        Text(
          text = "偏好設定",
          style = MaterialTheme.typography.titleMedium,
          fontWeight = FontWeight.SemiBold,
        )

        Row(
          modifier = Modifier.fillMaxWidth(),
          horizontalArrangement = Arrangement.SpaceBetween,
          verticalAlignment = Alignment.CenterVertically,
        ) {
          Column(modifier = Modifier.weight(1f)) {
            Text("Caps Lock 切換中／歐模式", style = MaterialTheme.typography.bodyMedium)
            Text(
              "啟用時會抑制硬體大寫燈號，避免許氏鍵位誤轉大寫",
              style = MaterialTheme.typography.bodySmall,
              color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
          }
          Switch(
            checked = settings.capsLockToggleMode,
            onCheckedChange = { checked ->
              updateSettings { it.copy(capsLockToggleMode = checked) }
            },
          )
        }

        Row(
          modifier = Modifier.fillMaxWidth(),
          horizontalArrangement = Arrangement.SpaceBetween,
          verticalAlignment = Alignment.CenterVertically,
        ) {
          Column(modifier = Modifier.weight(1f)) {
            Text("Alt + Space 備用中／歐切換", style = MaterialTheme.typography.bodyMedium)
            Text(
              "部分藍牙鍵盤無獨立 Caps Lock 時的備用組合鍵",
              style = MaterialTheme.typography.bodySmall,
              color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
          }
          Switch(
            checked = settings.altSpaceToggleMode,
            onCheckedChange = { checked ->
              updateSettings { it.copy(altSpaceToggleMode = checked) }
            },
          )
        }

        HorizontalDivider()

        Text("Shift + Space 行為", style = MaterialTheme.typography.labelLarge)
        FlowRow(
          horizontalArrangement = Arrangement.spacedBy(8.dp),
          verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
          listOf(
              RuhiImeSettings.SHIFT_SPACE_PASSTHROUGH to "禮讓系統（預設）",
              RuhiImeSettings.SHIFT_SPACE_TOGGLE_MODE to "切換中／歐",
              RuhiImeSettings.SHIFT_SPACE_TOGGLE_FULLWIDTH to "切換全／半形",
            )
            .forEach { (id, label) ->
              FilterChip(
                selected = settings.shiftSpaceAction == id,
                onClick = { updateSettings { it.copy(shiftSpaceAction = id) } },
                label = { Text(label) },
              )
            }
        }
        if (settings.shiftSpaceAction != RuhiImeSettings.SHIFT_SPACE_PASSTHROUGH) {
          Card(
            colors =
              CardDefaults.cardColors(
                containerColor = MaterialTheme.colorScheme.secondaryContainer
              )
          ) {
            Text(
              text =
                "提醒：Samsung One UI 預設會在系統層攔截 Shift + Space。\n" +
                  "請前往「設定 → 一般管理 → 實體鍵盤 → 變更語言的捷徑」取消勾選 Shift + Space。",
              modifier = Modifier.padding(10.dp),
              style = MaterialTheme.typography.bodySmall,
            )
          }
        }

        HorizontalDivider()

        Text("歐文子語言套件", style = MaterialTheme.typography.labelLarge)
        FlowRow(
          horizontalArrangement = Arrangement.spacedBy(8.dp),
          verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
          listOf(
              "en" to "English (en)",
              "sa_am" to "梵語轉寫 (sa_am)",
              "fr" to "Français (fr)",
              "de" to "Deutsch (de)",
              "es" to "Español (es)",
            )
            .forEach { (code, label) ->
              val checked = settings.westernLanguages.contains(code)
              FilterChip(
                selected = checked,
                onClick = {
                  if (code == "en" && checked && settings.westernLanguages.size <= 1) {
                    return@FilterChip
                  }
                  updateSettings { cur ->
                    val next = cur.westernLanguages.toMutableSet()
                    if (checked) next.remove(code) else next.add(code)
                    if (next.isEmpty()) next.add("en")
                    cur.copy(westernLanguages = next)
                  }
                },
                label = { Text(label) },
              )
            }
        }

        HorizontalDivider()

        Text("候選窗快選鍵", style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
          listOf("auto" to "自動（統一許氏 asdfghjkl）", "1234" to "統一數字 1–9")
            .forEach { (id, label) ->
              FilterChip(
                selected = settings.candidateQuickKey == id,
                onClick = { updateSettings { it.copy(candidateQuickKey = id) } },
                label = { Text(label) },
              )
            }
        }

        Text("許氏半形模式標點", style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
          listOf("fullwidth" to "中文全形標點（預設）", "consistent" to "一致性半形標點")
            .forEach { (id, label) ->
              FilterChip(
                selected = settings.halfwidthPunctMode == id,
                onClick = { updateSettings { it.copy(halfwidthPunctMode = id) } },
                label = { Text(label) },
              )
            }
        }

        Text("Tab 鍵分工模式", style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
          listOf("associative" to "聯想補全優先（預設）", "boundary_cut" to "手動斷詞優先")
            .forEach { (id, label) ->
              FilterChip(
                selected = settings.tabKeyMode == id,
                onClick = { updateSettings { it.copy(tabKeyMode = id) } },
                label = { Text(label) },
              )
            }
        }
      }
    }

    // 4. 個人自訂詞庫編輯區
    Card(modifier = Modifier.fillMaxWidth()) {
      Column(
        modifier = Modifier.padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
      ) {
        Text(
          text = "個人自訂詞庫（user_dict.tsv）",
          style = MaterialTheme.typography.titleMedium,
          fontWeight = FontWeight.SemiBold,
        )
        Text(
          text = "格式為「詞彙<Tab>注音[<Tab>權重][<Tab>縮略碼]」。儲存後於下次組字自動熱重載，更新 APK 亦不會覆蓋此檔。",
          style = MaterialTheme.typography.bodySmall,
          color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedTextField(
          value = userDictText,
          onValueChange = { userDictText = it },
          modifier = Modifier.fillMaxWidth(),
          minLines = 4,
          maxLines = 8,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
          Button(
            onClick = {
              RuhiAssetManager.writeUserDict(context, userDictText)
              Toast.makeText(context, "已儲存個人詞庫", Toast.LENGTH_SHORT).show()
            }
          ) {
            Text("儲存個人詞庫")
          }
          OutlinedButton(
            onClick = { userDictText = RuhiAssetManager.readUserDict(context) }
          ) {
            Text("重新載入")
          }
        }
      }
    }

    Spacer(modifier = Modifier.height(16.dp))
  }
}
