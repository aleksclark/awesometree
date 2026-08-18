package dev.awesometree.mobile.ui.workspaces

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Circle
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.navigation.NavController
import dev.awesometree.mobile.data.ApiClient
import dev.awesometree.mobile.data.ConnectionStore
import dev.awesometree.mobile.data.WorkProfileInfo
import dev.awesometree.mobile.data.WorkSessionInfo
import kotlinx.coroutines.launch

private const val DEFAULT_PROFILE_ID = "default"

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WorkSessionsScreen(connectionStore: ConnectionStore, navController: NavController) {
    val connection = connectionStore.connection.collectAsState().value ?: return
    val client = remember(connection) { ApiClient(connection) }
    val scope = rememberCoroutineScope()

    var sessions by remember { mutableStateOf<List<WorkSessionInfo>>(emptyList()) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var showCreate by remember { mutableStateOf(false) }

    fun refresh() {
        scope.launch {
            loading = true
            error = null
            client.listWorkSessions()
                .onSuccess { sessions = it; loading = false }
                .onFailure { error = it.message; loading = false }
        }
    }

    LaunchedEffect(connection) { refresh() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Work Sessions") },
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
                sessions.isEmpty() -> {
                    Text(
                        "No work sessions",
                        modifier = Modifier.align(Alignment.Center),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                else -> {
                    LazyColumn(Modifier.fillMaxSize()) {
                        items(sessions, key = { it.workSessionId }) { ws ->
                            WorkSessionItem(
                                ws = ws,
                                onPause = {
                                    scope.launch {
                                        client.transitionWorkSession(ws.workSessionId, "paused")
                                            .onSuccess { refresh() }
                                            .onFailure { error = it.message }
                                    }
                                },
                                onResume = {
                                    scope.launch {
                                        client.transitionWorkSession(ws.workSessionId, "open")
                                            .onSuccess { refresh() }
                                            .onFailure { error = it.message }
                                    }
                                },
                                onClose = {
                                    scope.launch {
                                        client.transitionWorkSession(ws.workSessionId, "closed")
                                            .onSuccess { refresh() }
                                            .onFailure { error = it.message }
                                    }
                                },
                                onDelete = {
                                    scope.launch {
                                        client.deleteWorkSession(ws.workSessionId)
                                            .onSuccess { refresh() }
                                            .onFailure { error = it.message }
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
        CreateWorkSessionDialog(
            client = client,
            onDismiss = { showCreate = false },
            onCreated = { showCreate = false; refresh() },
        )
    }
}

@Composable
private fun WorkSessionItem(
    ws: WorkSessionInfo,
    onPause: () -> Unit,
    onResume: () -> Unit,
    onClose: () -> Unit,
    onDelete: () -> Unit,
) {
    var expanded by remember { mutableStateOf(false) }
    var showConfirm by remember { mutableStateOf(false) }
    val isOpen = ws.state == "open"
    val isPaused = ws.state == "paused"
    Column {
        ListItem(
            headlineContent = { Text(ws.displayName ?: ws.workSessionId) },
            supportingContent = {
                Column {
                    Text("project: ${ws.projectId ?: "-"}  profile: ${ws.workProfileId ?: "-"}")
                    Text("state: ${ws.state}  ${ws.realizationStatus ?: ""}")
                }
            },
            leadingContent = {
                Icon(
                    Icons.Default.Circle,
                    contentDescription = ws.state,
                    tint = when (ws.state) {
                        "open" -> MaterialTheme.colorScheme.secondary
                        "paused" -> MaterialTheme.colorScheme.tertiary
                        else -> MaterialTheme.colorScheme.onSurfaceVariant
                    },
                    modifier = Modifier.size(12.dp),
                )
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
                when {
                    isOpen -> FilledTonalButton(onClick = onPause) {
                        Icon(Icons.Default.Pause, null, Modifier.size(18.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Pause")
                    }
                    isPaused -> Button(onClick = onResume) {
                        Icon(Icons.Default.PlayArrow, null, Modifier.size(18.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Resume")
                    }
                }
                if (isOpen || isPaused) {
                    FilledTonalButton(onClick = onClose) {
                        Icon(Icons.Default.Stop, null, Modifier.size(18.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Close")
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
            title = { Text("Delete work session?") },
            text = { Text("Delete \"${ws.workSessionId}\"? This cannot be undone.") },
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
private fun CreateWorkSessionDialog(
    client: ApiClient,
    onDismiss: () -> Unit,
    onCreated: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var selectedProject by remember { mutableStateOf("") }
    var projects by remember { mutableStateOf<List<String>>(emptyList()) }
    var profiles by remember { mutableStateOf<List<WorkProfileInfo>>(emptyList()) }
    var selectedProfile by remember { mutableStateOf(DEFAULT_PROFILE_ID) }
    var missingDefault by remember { mutableStateOf(false) }
    var creating by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var projectExpanded by remember { mutableStateOf(false) }
    var profileExpanded by remember { mutableStateOf(false) }

    LaunchedEffect(Unit) {
        client.listProjects()
            .onSuccess { list -> projects = list.map { it.projectId } }
            .onFailure { error = it.message }
        client.listWorkProfiles()
            .onSuccess { list ->
                profiles = list
                val hasDefault = list.any { it.workProfileId == DEFAULT_PROFILE_ID }
                missingDefault = !hasDefault
                selectedProfile = if (hasDefault) {
                    DEFAULT_PROFILE_ID
                } else {
                    list.firstOrNull()?.workProfileId ?: ""
                }
            }
            .onFailure { error = it.message }
    }

    val canSubmit = name.isNotBlank() &&
        selectedProject.isNotBlank() &&
        selectedProfile.isNotBlank() &&
        !missingDefault &&
        !creating

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Create Work Session") },
        text = {
            Column {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("work_session_id") },
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
                            IconButton(onClick = { projectExpanded = true }) {
                                Icon(Icons.Default.ArrowDropDown, "Select project")
                            }
                        },
                    )
                    DropdownMenu(expanded = projectExpanded, onDismissRequest = { projectExpanded = false }) {
                        projects.forEach { proj ->
                            DropdownMenuItem(
                                text = { Text(proj) },
                                onClick = {
                                    selectedProject = proj
                                    projectExpanded = false
                                },
                            )
                        }
                    }
                }
                Spacer(Modifier.height(8.dp))
                Box(Modifier.fillMaxWidth()) {
                    val profileLabel = profiles
                        .firstOrNull { it.workProfileId == selectedProfile }
                        ?.let { p ->
                            val dn = p.displayName ?: p.workProfileId
                            if (dn == p.workProfileId) dn else "$dn (${p.workProfileId})"
                        }
                        ?: selectedProfile
                    OutlinedTextField(
                        value = profileLabel,
                        onValueChange = {},
                        label = { Text("WorkProfile") },
                        readOnly = true,
                        singleLine = true,
                        enabled = !missingDefault,
                        modifier = Modifier.fillMaxWidth(),
                        trailingIcon = {
                            IconButton(onClick = { profileExpanded = true }, enabled = profiles.isNotEmpty()) {
                                Icon(Icons.Default.ArrowDropDown, "Select profile")
                            }
                        },
                    )
                    DropdownMenu(expanded = profileExpanded, onDismissRequest = { profileExpanded = false }) {
                        profiles.forEach { p ->
                            DropdownMenuItem(
                                text = {
                                    Text(
                                        buildString {
                                            append(p.displayName ?: p.workProfileId)
                                            if (p.workProfileId == DEFAULT_PROFILE_ID) append(" [default]")
                                            if ((p.displayName ?: "") != p.workProfileId) {
                                                append(" (${p.workProfileId})")
                                            }
                                        }
                                    )
                                },
                                onClick = {
                                    selectedProfile = p.workProfileId
                                    profileExpanded = false
                                },
                            )
                        }
                    }
                }
                if (missingDefault) {
                    Spacer(Modifier.height(8.dp))
                    Text(
                        "WorkProfile with work_profile_id exactly \"default\" is missing in Switchboard. Create it before opening sessions without an explicit profile.",
                        color = MaterialTheme.colorScheme.error,
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
                error?.let {
                    Spacer(Modifier.height(8.dp))
                    Text(it, color = MaterialTheme.colorScheme.error)
                }
            }
        },
        confirmButton = {
            TextButton(
                enabled = canSubmit,
                onClick = {
                    creating = true
                    scope.launch {
                        client.createWorkSession(
                            workSessionId = name.trim(),
                            projectId = selectedProject,
                            workProfileId = selectedProfile.takeIf { it.isNotBlank() },
                        )
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
