package dev.awesometree.mobile.ui.workspaces

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Circle
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.awesometree.mobile.data.ApiClient
import dev.awesometree.mobile.data.ConnectionStore
import dev.awesometree.mobile.data.WorkspaceInfo
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkspacesScreen(connectionStore: ConnectionStore) {
    val connection = connectionStore.connection.collectAsState().value ?: return
    val client = remember(connection) { ApiClient(connection) }
    val scope = rememberCoroutineScope()

    var workspaces by remember { mutableStateOf<List<WorkspaceInfo>>(emptyList()) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var showCreate by remember { mutableStateOf(false) }

    fun refresh() {
        scope.launch {
            loading = true
            error = null
            client.listWorkspaces()
                .onSuccess { workspaces = it; loading = false }
                .onFailure { error = it.message; loading = false }
        }
    }

    LaunchedEffect(connection) { refresh() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Workspaces") },
                actions = {
                    IconButton(onClick = { refresh() }) {
                        Icon(Icons.Default.Refresh, "Refresh")
                    }
                    IconButton(onClick = { showCreate = true }) {
                        Icon(Icons.Default.Add, "Create")
                    }
                },
            )
        },
    ) { padding ->
        Box(Modifier.fillMaxSize().padding(padding)) {
            when {
                loading -> CircularProgressIndicator(Modifier.align(Alignment.Center))
                error != null -> {
                    Column(
                        Modifier.align(Alignment.Center).padding(16.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        Text(error!!, color = MaterialTheme.colorScheme.error)
                        Spacer(Modifier.height(8.dp))
                        Button(onClick = { refresh() }) { Text("Retry") }
                    }
                }
                workspaces.isEmpty() -> {
                    Text(
                        "No workspaces",
                        modifier = Modifier.align(Alignment.Center),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                else -> {
                    LazyColumn(Modifier.fillMaxSize()) {
                        items(workspaces, key = { it.name }) { ws ->
                            WorkspaceItem(ws) {
                                scope.launch {
                                    client.deleteWorkspace(ws.name)
                                        .onSuccess { refresh() }
                                        .onFailure { error = it.message }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if (showCreate) {
        CreateWorkspaceDialog(
            client = client,
            onDismiss = { showCreate = false },
            onCreated = { showCreate = false; refresh() },
        )
    }
}

@Composable
private fun WorkspaceItem(ws: WorkspaceInfo, onDelete: () -> Unit) {
    var showConfirm by remember { mutableStateOf(false) }

    ListItem(
        headlineContent = { Text(ws.name) },
        supportingContent = { Text("project: ${ws.project}") },
        leadingContent = {
            Icon(
                Icons.Default.Circle,
                contentDescription = if (ws.active) "Active" else "Inactive",
                tint = if (ws.active)
                    MaterialTheme.colorScheme.secondary
                else
                    MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(12.dp),
            )
        },
        trailingContent = {
            IconButton(onClick = { showConfirm = true }) {
                Icon(Icons.Default.Delete, "Delete")
            }
        },
    )

    if (showConfirm) {
        AlertDialog(
            onDismissRequest = { showConfirm = false },
            title = { Text("Delete workspace?") },
            text = { Text("Delete \"${ws.name}\"? This cannot be undone.") },
            confirmButton = {
                TextButton(onClick = { showConfirm = false; onDelete() }) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { showConfirm = false }) { Text("Cancel") }
            },
        )
    }
}

@Composable
private fun CreateWorkspaceDialog(
    client: ApiClient,
    onDismiss: () -> Unit,
    onCreated: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var project by remember { mutableStateOf("") }
    var creating by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Create Workspace") },
        text = {
            Column {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Name") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(
                    value = project,
                    onValueChange = { project = it },
                    label = { Text("Project") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                error?.let {
                    Spacer(Modifier.height(8.dp))
                    Text(it, color = MaterialTheme.colorScheme.error)
                }
            }
        },
        confirmButton = {
            TextButton(
                enabled = name.isNotBlank() && project.isNotBlank() && !creating,
                onClick = {
                    creating = true
                    scope.launch {
                        client.createWorkspace(name.trim(), project.trim())
                            .onSuccess { onCreated() }
                            .onFailure { error = it.message; creating = false }
                    }
                },
            ) {
                if (creating) CircularProgressIndicator(Modifier.size(16.dp))
                else Text("Create")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}
