package dev.awesometree.mobile.ui.acp

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.awesometree.mobile.data.ApiClient
import dev.awesometree.mobile.data.ConnectionStore
import dev.awesometree.mobile.data.WorkspaceInfo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

data class ChatMessage(
    val role: String,
    val content: String,
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AcpScreen(connectionStore: ConnectionStore, preselectedWorkspace: String? = null) {
    val connection = connectionStore.connection.collectAsState().value ?: return
    val client = remember(connection) { ApiClient(connection) }
    val scope = rememberCoroutineScope()

    var workspaces by remember { mutableStateOf<List<WorkspaceInfo>>(emptyList()) }
    var selectedWorkspace by remember { mutableStateOf<WorkspaceInfo?>(null) }
    var messages by remember { mutableStateOf<List<ChatMessage>>(emptyList()) }
    var input by remember { mutableStateOf("") }
    var sending by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var expanded by remember { mutableStateOf(false) }
    var sessionId by remember { mutableStateOf<String?>(null) }

    val listState = rememberLazyListState()

    LaunchedEffect(connection) {
        client.listWorkspaces()
            .onSuccess { all ->
                workspaces = all.filter { it.active && (it.acpUrl != null || it.acpPort != null) }
                if (selectedWorkspace == null && workspaces.isNotEmpty()) {
                    val target = if (preselectedWorkspace != null) {
                        workspaces.find { it.name == preselectedWorkspace }
                    } else null
                    selectedWorkspace = target ?: workspaces.first()
                }
            }
    }

    fun loadHistory(ws: WorkspaceInfo) {
        sessionId = ws.acpSessionId
        scope.launch {
            try {
                val history = withContext(Dispatchers.IO) {
                    val url = URL("${connection.baseUrl}/api/acp/${URLEncoder.encode(ws.name, "UTF-8")}/history")
                    val conn = (url.openConnection() as HttpURLConnection).apply {
                        requestMethod = "GET"
                        setRequestProperty("Authorization", "Bearer ${connection.token}")
                        connectTimeout = 10_000
                        readTimeout = 10_000
                    }
                    val code = conn.responseCode
                    if (code !in 200..299) return@withContext emptyList<ChatMessage>()
                    val text = conn.inputStream.let {
                        BufferedReader(InputStreamReader(it)).use { r -> r.readText() }
                    }
                    val arr = org.json.JSONArray(text)
                    (0 until arr.length()).map { i ->
                        val obj = arr.getJSONObject(i)
                        ChatMessage(obj.getString("role"), obj.getString("content"))
                    }
                }
                messages = history
                if (messages.isNotEmpty()) {
                    listState.animateScrollToItem(messages.size - 1)
                }
            } catch (_: Exception) {
                messages = emptyList()
            }
        }
    }

    LaunchedEffect(selectedWorkspace) {
        selectedWorkspace?.let { loadHistory(it) }
    }

    fun sendMessage(ws: WorkspaceInfo, text: String) {
        sending = true
        error = null
        messages = messages + ChatMessage("user", text)
        messages = messages + ChatMessage("agent", "")

        scope.launch {
            try {
                withContext(Dispatchers.IO) {
                    val body = JSONObject().apply {
                        put("message", text)
                        sessionId?.let { put("session_id", it) }
                    }.toString()

                    val url = URL("${connection.baseUrl}/api/acp/${URLEncoder.encode(ws.name, "UTF-8")}/stream")
                    val conn = (url.openConnection() as HttpURLConnection).apply {
                        requestMethod = "POST"
                        setRequestProperty("Authorization", "Bearer ${connection.token}")
                        setRequestProperty("Content-Type", "application/json")
                        setRequestProperty("Accept", "application/x-ndjson")
                        connectTimeout = 10_000
                        readTimeout = 300_000
                        doOutput = true
                        outputStream.bufferedWriter().use { it.write(body) }
                    }

                    val code = conn.responseCode
                    if (code !in 200..299) {
                        val errText = conn.errorStream?.let {
                            BufferedReader(InputStreamReader(it)).use { r -> r.readText() }
                        } ?: ""
                        throw Exception("HTTP $code: $errText")
                    }

                    val reader = BufferedReader(InputStreamReader(conn.inputStream))
                    var line: String?
                    while (reader.readLine().also { line = it } != null) {
                        val l = line?.trim() ?: continue
                        if (l.isEmpty()) continue
                        val event = try { JSONObject(l) } catch (_: Exception) { continue }
                        val type = event.optString("type", "")

                        if (type == "message.part") {
                            val part = event.optJSONObject("part")
                            val content = part?.optString("content", "") ?: ""
                            if (content.isNotEmpty()) {
                                withContext(Dispatchers.Main) {
                                    val last = messages.last()
                                    messages = messages.dropLast(1) + last.copy(content = last.content + content)
                                    if (messages.isNotEmpty()) {
                                        listState.animateScrollToItem(messages.size - 1)
                                    }
                                }
                            }
                        }

                        val run = event.optJSONObject("run")
                        if (run != null) {
                            val sid = run.optString("session_id", "")
                            if (sid.isNotEmpty()) {
                                withContext(Dispatchers.Main) {
                                    sessionId = sid
                                }
                            }
                        }
                    }
                    reader.close()
                }

                sending = false
                val last = messages.lastOrNull()
                if (last != null && last.role == "agent" && last.content.isEmpty()) {
                    messages = messages.dropLast(1) + last.copy(content = "(no response)")
                }
            } catch (e: Exception) {
                error = e.message
                sending = false
                val last = messages.lastOrNull()
                if (last != null && last.role == "agent" && last.content.isEmpty()) {
                    messages = messages.dropLast(1)
                }
            }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Box {
                        TextButton(onClick = { expanded = true }) {
                            Text(
                                selectedWorkspace?.name ?: "Select workspace",
                                color = MaterialTheme.colorScheme.onSurface,
                            )
                        }
                        DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                            workspaces.forEach { ws ->
                                DropdownMenuItem(
                                    text = { Text(ws.name) },
                                    onClick = {
                                        selectedWorkspace = ws
                                        expanded = false
                                    },
                                )
                            }
                            if (workspaces.isEmpty()) {
                                DropdownMenuItem(
                                    text = { Text("No active workspaces with ACP") },
                                    enabled = false,
                                    onClick = {},
                                )
                            }
                        }
                    }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            if (selectedWorkspace == null) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text(
                        "No active workspaces with ACP",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            } else {
                LazyColumn(
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                    state = listState,
                    contentPadding = PaddingValues(16.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(messages) { msg ->
                        MessageBubble(msg)
                    }
                }

                if (sending) {
                    LinearProgressIndicator(
                        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
                    )
                }

                error?.let {
                    Text(
                        it,
                        color = MaterialTheme.colorScheme.error,
                        modifier = Modifier.padding(horizontal = 16.dp),
                        style = MaterialTheme.typography.bodySmall,
                    )
                }

                Row(
                    Modifier
                        .fillMaxWidth()
                        .padding(8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    OutlinedTextField(
                        value = input,
                        onValueChange = { input = it },
                        modifier = Modifier.weight(1f),
                        placeholder = { Text("Message agent...") },
                        maxLines = 4,
                    )
                    Spacer(Modifier.width(8.dp))
                    IconButton(
                        enabled = input.isNotBlank() && !sending && selectedWorkspace != null,
                        onClick = {
                            val ws = selectedWorkspace ?: return@IconButton
                            val text = input.trim()
                            input = ""
                            sendMessage(ws, text)
                        },
                    ) {
                        Icon(Icons.AutoMirrored.Filled.Send, "Send")
                    }
                }
            }
        }
    }
}

@Composable
private fun MessageBubble(msg: ChatMessage) {
    val isUser = msg.role == "user"
    val bgColor = if (isUser)
        MaterialTheme.colorScheme.primaryContainer
    else
        MaterialTheme.colorScheme.surfaceVariant
    val textColor = if (isUser)
        MaterialTheme.colorScheme.onPrimaryContainer
    else
        MaterialTheme.colorScheme.onSurfaceVariant
    val alignment = if (isUser) Alignment.End else Alignment.Start

    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = alignment,
    ) {
        Text(
            if (isUser) "You" else "Agent",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Surface(
            shape = MaterialTheme.shapes.medium,
            color = bgColor,
            modifier = Modifier.widthIn(max = 300.dp),
        ) {
            Text(
                msg.content,
                modifier = Modifier.padding(12.dp),
                color = textColor,
            )
        }
    }
}
