package org.ruhos.ruhi.ime.ui

import android.content.Context
import android.content.res.Configuration
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.TextUtils
import android.text.style.BackgroundColorSpan
import android.text.style.ForegroundColorSpan
import android.text.style.UnderlineSpan
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.HorizontalScrollView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import org.ruhos.ruhi.ime.nativebridge.EngineSnapshot

class InputDeskView(
  context: Context,
  private val onToggleMode: () -> Unit,
  private val onToggleFullwidth: () -> Unit,
  private val onCandidateClick: (Int) -> Unit,
  private val onPagePrev: () -> Unit,
  private val onPageNext: () -> Unit,
  private val onCommit: () -> Unit,
  private val onReset: () -> Unit,
  private val onOpenSettings: () -> Unit,
) : LinearLayout(context) {

  private val modeBadgeView: TextView
  private val widthBadgeView: TextView
  private val deskScrollView: HorizontalScrollView
  private val deskTextView: TextView
  private val commitBtn: TextView
  private val resetBtn: TextView
  private val settingsBtn: TextView

  private val candidateRow: LinearLayout
  private val candidateScrollView: HorizontalScrollView
  private val candidateContainer: LinearLayout
  private val pagePrevBtn: TextView
  private val pageLabelView: TextView
  private val pageNextBtn: TextView

  private val palette: ThemePalette

  private data class ThemePalette(
    val barBg: Int,
    val barBorder: Int,
    val hsuAccent: Int,
    val englishAccent: Int,
    val pillBg: Int,
    val pillActiveBg: Int,
    val textPrimary: Int,
    val textMuted: Int,
    val textOnAccent: Int,
    val selectionBg: Int,
  )

  init {
    val isDark =
      (context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) ==
        Configuration.UI_MODE_NIGHT_YES
    palette =
      if (isDark) {
        ThemePalette(
          barBg = Color.parseColor("#181A1F"),
          barBorder = Color.parseColor("#2E3440"),
          hsuAccent = Color.parseColor("#3B82F6"),
          englishAccent = Color.parseColor("#0D9488"),
          pillBg = Color.parseColor("#262A33"),
          pillActiveBg = Color.parseColor("#2563EB"),
          textPrimary = Color.parseColor("#F1F5F9"),
          textMuted = Color.parseColor("#94A3B8"),
          textOnAccent = Color.WHITE,
          selectionBg = Color.parseColor("#334155"),
        )
      } else {
        ThemePalette(
          barBg = Color.parseColor("#F8FAFC"),
          barBorder = Color.parseColor("#CBD5E1"),
          hsuAccent = Color.parseColor("#1D4ED8"),
          englishAccent = Color.parseColor("#0F766E"),
          pillBg = Color.parseColor("#E2E8F0"),
          pillActiveBg = Color.parseColor("#1D4ED8"),
          textPrimary = Color.parseColor("#0F172A"),
          textMuted = Color.parseColor("#64748B"),
          textOnAccent = Color.WHITE,
          selectionBg = Color.parseColor("#BFDBFE"),
        )
      }

    orientation = VERTICAL
    setBackgroundColor(palette.barBg)

    val topDivider =
      View(context).apply {
        layoutParams = LayoutParams(LayoutParams.MATCH_PARENT, dp(1))
        setBackgroundColor(palette.barBorder)
      }
    addView(topDivider)

    val contentWrap =
      LinearLayout(context).apply {
        orientation = VERTICAL
        setPadding(dp(8), dp(4), dp(8), dp(4))
      }

    // 底部避讓系統手勢導覽列（Galaxy Z Flip 5）
    ViewCompat.setOnApplyWindowInsetsListener(this) { _, insets ->
      val nav = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
      contentWrap.setPadding(dp(8), dp(4), dp(8), dp(4) + nav.bottom)
      insets
    }

    // Row 1: [許/歐] [半/全] | 輸入座組字句 (水平捲動) | [送] [✕] / [⚙]
    val row1 =
      LinearLayout(context).apply {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        minimumHeight = dp(34)
      }

    modeBadgeView =
      makePill("許", palette.hsuAccent, palette.textOnAccent, bold = true).apply {
        setOnClickListener { onToggleMode() }
      }
    widthBadgeView =
      makePill("半", palette.pillBg, palette.textPrimary, bold = false).apply {
        setOnClickListener { onToggleFullwidth() }
      }

    row1.addView(modeBadgeView, rowMarginRight(4))
    row1.addView(widthBadgeView, rowMarginRight(6))

    deskTextView =
      TextView(context).apply {
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
        setTextColor(palette.textMuted)
        maxLines = 1
        setSingleLine(true)
        text = "如喜詞庫載入中…"
      }
    deskScrollView =
      HorizontalScrollView(context).apply {
        isHorizontalScrollBarEnabled = false
        isFillViewport = true
        addView(
          deskTextView,
          LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT).apply {
            gravity = Gravity.CENTER_VERTICAL
          },
        )
      }
    row1.addView(
      deskScrollView,
      LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f).apply { rightMargin = dp(6) },
    )

    commitBtn =
      makePill("送", palette.pillBg, palette.textPrimary, bold = true).apply {
        visibility = GONE
        setOnClickListener { onCommit() }
      }
    resetBtn =
      makePill("✕", palette.pillBg, palette.textMuted, bold = false).apply {
        visibility = GONE
        setOnClickListener { onReset() }
      }
    settingsBtn =
      makePill("⚙", palette.pillBg, palette.textMuted, bold = false).apply {
        setOnClickListener { onOpenSettings() }
      }

    row1.addView(commitBtn, rowMarginRight(4))
    row1.addView(resetBtn)
    row1.addView(settingsBtn)
    contentWrap.addView(row1, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))

    // Row 2: 候選字列（有候選字或選單時展開，約 36dp）
    candidateRow =
      LinearLayout(context).apply {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        visibility = GONE
        minimumHeight = dp(34)
      }
    candidateContainer =
      LinearLayout(context).apply {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
      }
    candidateScrollView =
      HorizontalScrollView(context).apply {
        isHorizontalScrollBarEnabled = false
        addView(candidateContainer)
      }
    candidateRow.addView(
      candidateScrollView,
      LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f).apply { rightMargin = dp(4) },
    )

    pagePrevBtn =
      makePill("◀", palette.pillBg, palette.textPrimary, bold = false).apply {
        setOnClickListener { onPagePrev() }
      }
    pageLabelView =
      TextView(context).apply {
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 11f)
        setTextColor(palette.textMuted)
        maxLines = 1
        setPadding(dp(4), 0, dp(4), 0)
      }
    pageNextBtn =
      makePill("▶", palette.pillBg, palette.textPrimary, bold = false).apply {
        setOnClickListener { onPageNext() }
      }
    candidateRow.addView(pagePrevBtn, rowMarginRight(3))
    candidateRow.addView(pageLabelView)
    candidateRow.addView(pageNextBtn)

    contentWrap.addView(
      candidateRow,
      LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT).apply {
        topMargin = dp(4)
      },
    )

    addView(contentWrap, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
  }

  fun renderLoading(loading: Boolean) {
    if (loading) {
      deskTextView.setTextColor(palette.textMuted)
      deskTextView.text = "如喜詞庫載入中…"
      candidateRow.visibility = GONE
    }
  }

  fun render(snapshot: EngineSnapshot?, isReady: Boolean, isPasswordField: Boolean) {
    if (!isReady) {
      renderLoading(true)
      return
    }
    val snap = snapshot ?: return

    // 模式徽章 [許] / [歐]
    modeBadgeView.text = snap.modeBadge
    val accentColor =
      if (snap.keyboardMode == "english") palette.englishAccent else palette.hsuAccent
    modeBadgeView.background = roundedBg(accentColor, dp(8).toFloat())
    modeBadgeView.setTextColor(palette.textOnAccent)

    // 全半形徽章 [半] / [全]
    widthBadgeView.text = if (snap.fullwidthMode) "全" else "半"
    if (snap.fullwidthMode) {
      widthBadgeView.background = roundedBg(palette.pillActiveBg, dp(8).toFloat())
      widthBadgeView.setTextColor(palette.textOnAccent)
    } else {
      widthBadgeView.background = roundedBg(palette.pillBg, dp(8).toFloat())
      widthBadgeView.setTextColor(palette.textPrimary)
    }

    if (snap.hasBuffer) {
      commitBtn.visibility = VISIBLE
      resetBtn.visibility = VISIBLE
      settingsBtn.visibility = GONE
      deskTextView.setTextColor(palette.textPrimary)
      deskTextView.text = formatCompositionSpannable(snap, accentColor)
      deskScrollView.post {
        val layout = deskTextView.layout ?: return@post
        val targetChar = snap.cursorPos.coerceIn(0, deskTextView.text.length)
        val x = layout.getPrimaryHorizontal(targetChar).toInt()
        val targetScroll = (x - deskScrollView.width / 2).coerceAtLeast(0)
        deskScrollView.smoothScrollTo(targetScroll, 0)
      }
    } else {
      commitBtn.visibility = GONE
      resetBtn.visibility = GONE
      settingsBtn.visibility = VISIBLE
      deskTextView.setTextColor(palette.textMuted)
      deskTextView.text =
        when {
          isPasswordField -> "密碼欄位安全直通中"
          snap.keyboardMode == "english" -> "歐文就緒 · Caps Lock 切換許氏"
          else -> "許氏就緒 · Caps Lock 切換歐文"
        }
    }

    if (snap.showCandidates && snap.candidates.isNotEmpty()) {
      candidateRow.visibility = VISIBLE
      renderCandidates(snap)
    } else {
      candidateRow.visibility = GONE
      candidateContainer.removeAllViews()
    }
  }

  private fun codePointOffsetToCharIndex(text: String, cpOffset: Int): Int {
    val count = text.codePointCount(0, text.length)
    val clamped = cpOffset.coerceIn(0, count)
    return text.offsetByCodePoints(0, clamped)
  }

  private fun formatCompositionSpannable(snap: EngineSnapshot, accentColor: Int): CharSequence {
    val composed = snap.composed
    val preedit = snap.preedit
    val sb = SpannableStringBuilder()
    val safeCursor = codePointOffsetToCharIndex(composed, snap.cursorPos)

    if (composed.isNotEmpty()) {
      sb.append(composed)
      if (snap.selectionStart >= 0 && snap.selectionEnd > snap.selectionStart) {
        val s = codePointOffsetToCharIndex(composed, snap.selectionStart)
        val e = codePointOffsetToCharIndex(composed, snap.selectionEnd).coerceAtLeast(s)
        if (e > s) {
          sb.setSpan(
            BackgroundColorSpan(palette.selectionBg),
            s,
            e,
            Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
          )
        }
      }
    }

    if (preedit.isNotEmpty()) {
      val matchesAtCursor =
        safeCursor + preedit.length <= sb.length &&
          composed.regionMatches(safeCursor, preedit, 0, preedit.length)
      if (matchesAtCursor) {
        sb.setSpan(
          UnderlineSpan(),
          safeCursor,
          safeCursor + preedit.length,
          Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        sb.setSpan(
          ForegroundColorSpan(accentColor),
          safeCursor,
          safeCursor + preedit.length,
          Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
      } else if (!composed.contains(preedit)) {
        sb.insert(safeCursor, preedit)
        sb.setSpan(
          UnderlineSpan(),
          safeCursor,
          safeCursor + preedit.length,
          Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
        sb.setSpan(
          ForegroundColorSpan(accentColor),
          safeCursor,
          safeCursor + preedit.length,
          Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
        )
      }
    } else {
      val markerPos = safeCursor.coerceIn(0, sb.length)
      sb.insert(markerPos, "|")
      sb.setSpan(
        ForegroundColorSpan(accentColor),
        markerPos,
        markerPos + 1,
        Spanned.SPAN_EXCLUSIVE_EXCLUSIVE,
      )
    }

    return sb
  }

  private fun renderCandidates(snap: EngineSnapshot) {
    candidateContainer.removeAllViews()
    val pageSize = snap.pageSize.coerceAtLeast(1)
    val total = snap.candidates.size
    val totalPages = ((total + pageSize - 1) / pageSize).coerceAtLeast(1)
    val activeIdx = snap.candidateIdx.coerceIn(0, total - 1)
    val curPage = activeIdx / pageSize
    val pageStart = curPage * pageSize
    val pageEnd = (pageStart + pageSize).coerceAtMost(total)

    val asdfLabels = arrayOf("a", "s", "d", "f", "g", "h", "j", "k", "l")
    var activeView: View? = null

    for (i in pageStart until pageEnd) {
      val localIdx = i - pageStart
      val candText = snap.candidates[i]
      val label =
        if (snap.quickSelectActive) {
          val keyHint =
            if (snap.quickKeyMode == "asdf" && localIdx < asdfLabels.size) {
              asdfLabels[localIdx]
            } else {
              (localIdx + 1).toString()
            }
          "$keyHint $candText"
        } else {
          candText
        }

      val isActive = (i == activeIdx)
      val bg = if (isActive) palette.pillActiveBg else palette.pillBg
      val fg = if (isActive) palette.textOnAccent else palette.textPrimary

      val pill =
        makePill(label, bg, fg, bold = isActive).apply {
          maxLines = 1
          setSingleLine(true)
          ellipsize = TextUtils.TruncateAt.END
          maxWidth = dp(220)
          setOnClickListener { onCandidateClick(i) }
        }
      if (isActive) {
        activeView = pill
      }
      candidateContainer.addView(pill, rowMarginRight(4))
    }

    if (totalPages > 1) {
      pagePrevBtn.visibility = VISIBLE
      pageLabelView.visibility = VISIBLE
      pageNextBtn.visibility = VISIBLE
      pageLabelView.text = "${curPage + 1}/$totalPages"
    } else {
      pagePrevBtn.visibility = GONE
      pageLabelView.visibility = GONE
      pageNextBtn.visibility = GONE
    }

    activeView?.let { target ->
      candidateScrollView.post {
        val x = (target.left - candidateScrollView.width / 3).coerceAtLeast(0)
        candidateScrollView.smoothScrollTo(x, 0)
      }
    }
  }

  private fun makePill(label: String, bgColor: Int, fgColor: Int, bold: Boolean): TextView {
    return TextView(context).apply {
      text = label
      setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
      setTextColor(fgColor)
      typeface = if (bold) Typeface.DEFAULT_BOLD else Typeface.DEFAULT
      maxLines = 1
      setSingleLine(true)
      gravity = Gravity.CENTER
      setPadding(dp(9), dp(4), dp(9), dp(4))
      background = roundedBg(bgColor, dp(8).toFloat())
    }
  }

  private fun roundedBg(color: Int, radiusPx: Float): GradientDrawable {
    return GradientDrawable().apply {
      shape = GradientDrawable.RECTANGLE
      cornerRadius = radiusPx
      setColor(color)
    }
  }

  private fun rowMarginRight(rightDp: Int): LayoutParams {
    return LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT).apply {
      rightMargin = dp(rightDp)
    }
  }

  private fun dp(value: Int): Int {
    return TypedValue.applyDimension(
        TypedValue.COMPLEX_UNIT_DIP,
        value.toFloat(),
        resources.displayMetrics,
      )
      .toInt()
  }
}
