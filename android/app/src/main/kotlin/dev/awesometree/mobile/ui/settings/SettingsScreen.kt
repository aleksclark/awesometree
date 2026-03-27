package dev.awesometree.mobile.ui.settings

import android.Manifest
import android.content.pm.PackageManager
import android.util.Size
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.QrCodeScanner
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import dev.awesometree.mobile.data.ConnectionStore
import dev.awesometree.mobile.data.ServerConnection
import java.util.concurrent.Executors

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    connectionStore: ConnectionStore,
    fullScreen: Boolean = false,
) {
    val connection by connectionStore.connection.collectAsState()
    var host by remember { mutableStateOf(connection?.host ?: "") }
    var port by remember { mutableStateOf(connection?.port?.toString() ?: "9099") }
    var useHttps by remember { mutableStateOf(connection?.useHttps ?: false) }
    var scanning by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    fun parseHostInput(): Triple<String, String, Boolean> {
        val raw = host.trim()
        return when {
            raw.startsWith("https://") -> Triple(raw.removePrefix("https://"), port, true)
            raw.startsWith("http://") -> Triple(raw.removePrefix("http://"), port, false)
            else -> Triple(raw, port, useHttps)
        }
    }

    if (scanning) {
        QrScannerScreen(
            onScanned = { token ->
                scanning = false
                val (h, p, https) = parseHostInput()
                val portNum = p.toIntOrNull()
                if (h.isBlank() || portNum == null || portNum !in 1..65535) {
                    error = "Enter a valid host and port before scanning"
                    return@QrScannerScreen
                }
                connectionStore.save(ServerConnection(h, portNum, token.trim(), https))
                error = null
            },
            onCancel = { scanning = false },
        )
        return
    }

    Scaffold(
        topBar = {
            if (!fullScreen) {
                TopAppBar(title = { Text("Settings") })
            }
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = if (fullScreen) Arrangement.Center else Arrangement.Top,
        ) {
            if (fullScreen) {
                Text(
                    "awesometree",
                    style = MaterialTheme.typography.headlineLarge,
                    color = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.height(8.dp))
                Text(
                    "Enter your server address, then scan the QR code",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(32.dp))
            }

            OutlinedTextField(
                value = host,
                onValueChange = { host = it },
                label = { Text("Server host") },
                placeholder = { Text("192.168.1.100") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(
                value = port,
                onValueChange = { port = it.filter { c -> c.isDigit() } },
                label = { Text("Port") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            Spacer(Modifier.height(8.dp))
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Checkbox(
                    checked = useHttps,
                    onCheckedChange = { useHttps = it },
                )
                Text("Use HTTPS", style = MaterialTheme.typography.bodyMedium)
            }

            if (!useHttps && !host.startsWith("https://")) {
                Spacer(Modifier.height(4.dp))
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Icon(
                        Icons.Default.Warning,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.error,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        "HTTP is unencrypted. Use on trusted networks only.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            }

            Spacer(Modifier.height(16.dp))
            Button(
                onClick = { scanning = true },
                enabled = host.isNotBlank() && (port.toIntOrNull() ?: 0) in 1..65535,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Icon(Icons.Default.QrCodeScanner, null, Modifier.size(24.dp))
                Spacer(Modifier.width(8.dp))
                Text("Scan Token QR Code")
            }

            Spacer(Modifier.height(8.dp))
            var token by remember { mutableStateOf("") }
            OutlinedTextField(
                value = token,
                onValueChange = { token = it },
                label = { Text("Or paste token") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            OutlinedButton(
                onClick = {
                    val (h, p, https) = parseHostInput()
                    val portNum = p.toIntOrNull()
                    if (h.isNotBlank() && portNum != null && portNum in 1..65535 && token.isNotBlank()) {
                        connectionStore.save(ServerConnection(h, portNum, token.trim(), https))
                        error = null
                    } else {
                        error = "Fill in host, port, and token"
                    }
                },
                enabled = host.isNotBlank() && (port.toIntOrNull() ?: 0) in 1..65535 && token.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Text("Connect with Token")
            }

            error?.let {
                Spacer(Modifier.height(16.dp))
                Text(it, color = MaterialTheme.colorScheme.error)
            }

            connection?.let { conn ->
                Spacer(Modifier.height(24.dp))
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp)) {
                        Text("Connected", style = MaterialTheme.typography.titleMedium,
                            color = MaterialTheme.colorScheme.secondary)
                        Spacer(Modifier.height(8.dp))
                        Text(conn.baseUrl, style = MaterialTheme.typography.bodyMedium)
                    }
                }
                Spacer(Modifier.height(16.dp))
                OutlinedButton(
                    onClick = { connectionStore.clear() },
                    modifier = Modifier.fillMaxWidth(),
                    colors = ButtonDefaults.outlinedButtonColors(
                        contentColor = MaterialTheme.colorScheme.error,
                    ),
                ) {
                    Text("Disconnect")
                }
            }
        }
    }
}

@Composable
private fun QrScannerScreen(
    onScanned: (String) -> Unit,
    onCancel: () -> Unit,
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    var hasCameraPermission by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED
        )
    }

    val launcher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission()
    ) { granted ->
        hasCameraPermission = granted
        if (!granted) onCancel()
    }

    LaunchedEffect(Unit) {
        if (!hasCameraPermission) {
            launcher.launch(Manifest.permission.CAMERA)
        }
    }

    if (!hasCameraPermission) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Text("Camera permission required")
        }
        return
    }

    val scanned = remember { mutableStateOf(false) }

    Box(Modifier.fillMaxSize()) {
        AndroidView(
            factory = { ctx ->
                val previewView = PreviewView(ctx)
                val cameraProviderFuture = ProcessCameraProvider.getInstance(ctx)
                cameraProviderFuture.addListener({
                    val cameraProvider = cameraProviderFuture.get()
                    val preview = Preview.Builder().build().also {
                        it.surfaceProvider = previewView.surfaceProvider
                    }

                    val analyzer = ImageAnalysis.Builder()
                        .setTargetResolution(Size(1280, 720))
                        .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                        .build()

                    val scanner = BarcodeScanning.getClient()
                    val executor = Executors.newSingleThreadExecutor()

                    analyzer.setAnalyzer(executor) { imageProxy ->
                        @Suppress("UnsafeOptInUsageError")
                        val mediaImage = imageProxy.image
                        if (mediaImage != null && !scanned.value) {
                            val image = InputImage.fromMediaImage(
                                mediaImage,
                                imageProxy.imageInfo.rotationDegrees
                            )
                            scanner.process(image)
                                .addOnSuccessListener { barcodes ->
                                    for (barcode in barcodes) {
                                        if (barcode.valueType == Barcode.TYPE_TEXT) {
                                            barcode.rawValue?.let { value ->
                                                if (!scanned.value) {
                                                    scanned.value = true
                                                    onScanned(value)
                                                }
                                            }
                                        }
                                    }
                                }
                                .addOnCompleteListener { imageProxy.close() }
                        } else {
                            imageProxy.close()
                        }
                    }

                    try {
                        cameraProvider.unbindAll()
                        cameraProvider.bindToLifecycle(
                            lifecycleOwner,
                            CameraSelector.DEFAULT_BACK_CAMERA,
                            preview,
                            analyzer,
                        )
                    } catch (_: Exception) {}
                }, ContextCompat.getMainExecutor(ctx))
                previewView
            },
            modifier = Modifier.fillMaxSize(),
        )

        Column(
            Modifier
                .fillMaxWidth()
                .align(Alignment.TopCenter)
                .padding(16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Spacer(Modifier.height(48.dp))
            Text(
                "Scan the token QR code",
                color = MaterialTheme.colorScheme.onPrimary,
                style = MaterialTheme.typography.titleMedium,
            )
        }

        Button(
            onClick = onCancel,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(32.dp),
        ) {
            Text("Cancel")
        }
    }
}
