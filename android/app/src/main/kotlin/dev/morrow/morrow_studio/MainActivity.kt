package dev.morrow.morrow_studio

import io.flutter.embedding.android.FlutterActivity
import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.util.concurrent.Executors

class MainActivity : FlutterActivity() {
    private val exportRequest = 4107
    private var pendingExport: Pair<File, MethodChannel.Result>? = null
    private val fileWorker = Executors.newSingleThreadExecutor()

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "dev.morrow/files")
            .setMethodCallHandler { call, result ->
                if (call.method != "open" && call.method != "export") {
                    result.notImplemented()
                    return@setMethodCallHandler
                }
                try {
                    val file = File(requireNotNull(call.argument<String>("path"))).canonicalFile
                    val root = File(filesDir, "textures").canonicalFile
                    require(file.isFile && file.path.startsWith(root.path + File.separator)) {
                        "已保存的附件不存在，请重新导入。"
                    }
                    val name = call.argument<String>("name")
                        ?.substringAfterLast('/')?.substringAfterLast('\\')
                        ?.takeIf { it.isNotBlank() } ?: file.name
                    val mime = MimeTypeMap.getSingleton()
                        .getMimeTypeFromExtension(file.extension.lowercase())
                        ?: "application/octet-stream"
                    if (call.method == "open") {
                        val uri = FileProvider.getUriForFile(
                            this, "$packageName.attachments", file, name
                        )
                        startActivity(Intent(Intent.ACTION_VIEW).apply {
                            setDataAndType(uri, mime)
                            clipData = ClipData.newRawUri(name, uri)
                            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                        })
                        result.success(true)
                    } else if (pendingExport != null) {
                        result.error("busy", "请先完成当前另存操作。", null)
                    } else {
                        pendingExport = file to result
                        try {
                            startActivityForResult(Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                                addCategory(Intent.CATEGORY_OPENABLE)
                                type = mime
                                putExtra(Intent.EXTRA_TITLE, name)
                            }, exportRequest)
                        } catch (error: Exception) {
                            pendingExport = null
                            throw error
                        }
                    }
                } catch (error: android.content.ActivityNotFoundException) {
                    result.error("no_app", "没有可打开此文件的应用，请先另存附件。", null)
                } catch (error: Exception) {
                    result.error("file_error", "文件操作失败，请检查附件或存储空间。", null)
                }
            }
    }

    @Deprecated("Required for the document picker result")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != exportRequest) return
        val pending = pendingExport ?: return
        pendingExport = null
        val destination = data?.data
        if (resultCode != Activity.RESULT_OK || destination == null) {
            pending.second.success(false)
            return
        }
        // Large CAD/media attachments must not block the Android UI thread.
        fileWorker.execute {
            try {
                val output = contentResolver.openOutputStream(destination, "wt")
                    ?: error("Cannot write destination")
                output.use { sink -> pending.first.inputStream().use { it.copyTo(sink) } }
                runOnUiThread { pending.second.success(true) }
            } catch (error: Exception) {
                runOnUiThread {
                    pending.second.error("export_failed", "另存失败，请检查目标位置与可用空间。", null)
                }
            }
        }
    }

    override fun onDestroy() {
        pendingExport?.second?.error("cancelled", "文件操作已取消，请重试。", null)
        pendingExport = null
        fileWorker.shutdown()
        super.onDestroy()
    }
}
