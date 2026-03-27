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
import kotlinx.coroutines.launch

data class ChatMessage(
    val role: String,
    val content: String,
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AcpScreen(connectionStore: ConnectionStore) {
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

    val listState = rememberLazyListState()

    LaunchedEffect(connection) {
        client.listWorkspaces()
            .onSuccess { all ->
                workspaces = all.filter { it.active && it.acpPort != null }
                if (selectedWorkspace == null && workspaces.isNotEmpty()) {
                    selectedWorkspace = workspaces.first()
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
                                        messages = emptyList()
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
                            messages = messages + ChatMessage("user", text)
                            sending = true
                            error = null
                            scope.launch {
                                client.acpSend(ws.name, text)
                                    .onSuccess { response ->
                                        messages = messages + ChatMessage("assistant", response)
                                        sending = false
                                    }
                                    .onFailure {
                                        error = it.message
                                        sending = false
                                    }
                                listState.animateScrollToItem(messages.size)
                            }
                        },
                    ) {
                        if (sending) {
                            CircularProgressIndicator(Modifier.size(24.dp))
                        } else {
                            Icon(Icons.AutoMirrored.Filled.Send, "Send")
                        }
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
