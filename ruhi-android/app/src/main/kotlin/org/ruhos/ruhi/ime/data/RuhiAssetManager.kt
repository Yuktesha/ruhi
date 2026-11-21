package org.ruhos.ruhi.ime.data

import android.content.Context
import java.io.File
import java.io.FileOutputStream

object RuhiAssetManager {
  private const val ASSET_ROOT = "ruhi_data"
  private const val DATA_DIR_NAME = "ruhi_data"
  private const val MANIFEST_ASSET = "manifest_version.txt"
  private const val MANIFEST_LOCAL = ".manifest_version"

  private val CORE_ASSET_FILES =
    listOf(
      "lexicon_core.tsv",
      "lexicon_english.tsv",
      "transitions_user.tsv",
      "patterns_user.jsonl",
      "feature_weights.tsv",
      "symbols.tsv",
      "symbol_tree.tsv",
      "packs/pack_western_sanskrit.tsv",
    )

  private const val DEFAULT_USER_DICT_CONTENT =
    "# 如喜個人自訂詞庫（Tab 分隔：詞彙\\t注音[\\t權重][\\t自訂縮略碼]）\n"

  fun getDataDir(context: Context): File = File(context.filesDir, DATA_DIR_NAME)

  fun getUserDictFile(context: Context): File = File(getDataDir(context), "user_dict.tsv")

  fun ensureExtracted(context: Context): File {
    val targetDir = getDataDir(context)
    if (!targetDir.exists()) {
      targetDir.mkdirs()
    }

    val assetManifest = readAssetText(context, "$ASSET_ROOT/$MANIFEST_ASSET")?.trim().orEmpty()
    val localManifestFile = File(targetDir, MANIFEST_LOCAL)
    val localManifest =
      if (localManifestFile.exists()) localManifestFile.readText(Charsets.UTF_8).trim() else ""

    val coreMissing = CORE_ASSET_FILES.any { !File(targetDir, it).exists() }
    val needsSync = coreMissing || (assetManifest.isNotEmpty() && assetManifest != localManifest)

    if (needsSync) {
      for (rel in CORE_ASSET_FILES) {
        copyAssetAtomic(context, "$ASSET_ROOT/$rel", File(targetDir, rel))
      }
      if (assetManifest.isNotEmpty()) {
        val tmpManifest = File(targetDir, "$MANIFEST_LOCAL.tmp")
        tmpManifest.writeText(assetManifest, Charsets.UTF_8)
        if (!tmpManifest.renameTo(localManifestFile)) {
          tmpManifest.copyTo(localManifestFile, overwrite = true)
          tmpManifest.delete()
        }
      }
    }

    val templatesFile = File(targetDir, "templates.tsv")
    if (!templatesFile.exists()) {
      copyAssetAtomic(context, "$ASSET_ROOT/templates.tsv", templatesFile)
    }

    val userDictFile = getUserDictFile(context)
    if (!userDictFile.exists()) {
      userDictFile.writeText(DEFAULT_USER_DICT_CONTENT, Charsets.UTF_8)
    }

    return targetDir
  }

  fun readUserDict(context: Context): String {
    val dir = ensureExtracted(context)
    val file = File(dir, "user_dict.tsv")
    return if (file.exists()) file.readText(Charsets.UTF_8) else DEFAULT_USER_DICT_CONTENT
  }

  fun writeUserDict(context: Context, content: String) {
    val dir = ensureExtracted(context)
    val file = File(dir, "user_dict.tsv")
    val tmp = File(dir, "user_dict.tsv.tmp")
    tmp.writeText(content, Charsets.UTF_8)
    if (!tmp.renameTo(file)) {
      tmp.copyTo(file, overwrite = true)
      tmp.delete()
    }
  }

  private fun readAssetText(context: Context, path: String): String? {
    return try {
      context.assets.open(path).bufferedReader(Charsets.UTF_8).use { it.readText() }
    } catch (_: Exception) {
      null
    }
  }

  private fun copyAssetAtomic(context: Context, assetPath: String, destFile: File) {
    val parent = destFile.parentFile
    if (parent != null && !parent.exists()) {
      parent.mkdirs()
    }
    val tmp = File(destFile.parentFile, "${destFile.name}.tmp")
    context.assets.open(assetPath).use { input ->
      FileOutputStream(tmp).use { output ->
        input.copyTo(output, bufferSize = 32 * 1024)
        output.fd.sync()
      }
    }
    if (!tmp.renameTo(destFile)) {
      tmp.copyTo(destFile, overwrite = true)
      tmp.delete()
    }
  }
}
