package dev.awesometree.mobile.ui.workspaces

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.filled.Circle
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.awesometree.mobile.data.ApiClient
import dev.awesometree.mobile.data.ConnectionStore
import dev.awesometree.mobile.data.WorkspaceInfo
import kotlinx.coroutines.launch
import java.net.URLEncoder
import androidx.navigation.NavController

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkspacesScreen(connectionStore: ConnectionStore, navController: NavController) {
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
                            WorkspaceItem(
                                ws = ws,
                                onStart = {
                                    scope.launch {
                                        client.startWorkspace(ws.name)
                                            .onSuccess { refresh() }
                                            .onFailure { error = it.message }
                                    }
                                },
                                onStop = {
                                    scope.launch {
                                        client.stopWorkspace(ws.name)
                                            .onSuccess { refresh() }
                                            .onFailure { error = it.message }
                                    }
                                },
                                onDelete = {
                                    scope.launch {
                                        client.deleteWorkspace(ws.name)
                                            .onSuccess { refresh() }
                                            .onFailure { error = it.message }
                                    }
                                },
                                onAgent = {
                                    navController.navigate("acp/${URLEncoder.encode(ws.name, "UTF-8")}") {
                                        launchSingleTop = true
                                    }
                                },
                            )
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
private fun WorkspaceItem(
    ws: WorkspaceInfo,
    onStart: () -> Unit,
    onStop: () -> Unit,
    onDelete: () -> Unit,
    onAgent: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    var showConfirm by remember { mutableStateOf(false) }
    val hasAcp = ws.acpStatus != null
    val acpRunning = ws.acpStatus == "running"

    Column {
        ListItem(
            headlineContent = { Text(ws.name) },
            supportingContent = {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("project: ${ws.project}")
                    if (hasAcp) {
                        Spacer(Modifier.width(8.dp))
                        Text(
                            "ACP",
                            style = MaterialTheme.typography.labelSmall,
                            color = if (acpRunning)
                                MaterialTheme.colorScheme.primary
                            else
                                MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            },
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
                if (hasAcp) {
                    IconButton(
                        onClick = onAgent,
                        enabled = acpRunning,
                    ) {
                        Icon(
                            Icons.AutoMirrored.Filled.Chat,
                            contentDescription = "Agent",
                            tint = if (acpRunning)
                                MaterialTheme.colorScheme.primary
                            else
                                MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.38f),
                        )
                    }
                }
            },
            modifier = Modifier.clickable { expanded = !expanded },
        )

        AnimatedVisibility(visible = expanded) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(start = 40.dp, end = 16.dp, bottom = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                if (ws.active) {
                    FilledTonalButton(onClick = onStop) {
                        Icon(Icons.Default.Stop, null, Modifier.size(18.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Stop")
                    }
                } else {
                    Button(onClick = onStart) {
                        Icon(Icons.Default.PlayArrow, null, Modifier.size(18.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Start")
                    }
                }
                OutlinedButton(
                    onClick = { showConfirm = true },
                    colors = ButtonDefaults.outlinedButtonColors(
                        contentColor = MaterialTheme.colorScheme.error,
                    ),
                ) {
                    Icon(Icons.Default.Delete, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(4.dp))
                    Text("Delete")
                }
            }
        }
    }

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
    var selectedProject by remember { mutableStateOf("") }
    var projects by remember { mutableStateOf<List<String>>(emptyList()) }
    var creating by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var expanded by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        client.listProjects()
            .onSuccess { list -> projects = list.map { it.name } }
    }

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
                Box(Modifier.fillMaxWidth()) {
                    OutlinedTextField(
                        value = selectedProject,
                        onValueChange = {},
                        label = { Text("Project") },
                        readOnly = true,
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                        trailingIcon = {
                            IconButton(onClick = { expanded = true }) {
                                Icon(Icons.Default.ArrowDropDown, "Select project")
                            }
                        },
                    )
                    DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                        projects.forEach { proj ->
                            DropdownMenuItem(
                                text = { Text(proj) },
                                onClick = {
                                    selectedProject = proj
                                    expanded = false
                                },
                            )
                        }
                        if (projects.isEmpty()) {
                            DropdownMenuItem(
                                text = { Text("Loading...") },
                                enabled = false,
                                onClick = {},
                            )
                        }
                    }
                }
                error?.let {
                    Spacer(Modifier.height(8.dp))
                    Text(it, color = MaterialTheme.colorScheme.error)
                }
            }
        },
        confirmButton = {
            TextButton(
                enabled = name.isNotBlank() && selectedProject.isNotBlank() && !creating,
                onClick = {
                    creating = true
                    scope.launch {
                        client.createWorkspace(name.trim(), selectedProject)
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
