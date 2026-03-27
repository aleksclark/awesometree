package dev.awesometree.mobile.ui.projects

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.awesometree.mobile.data.ApiClient
import dev.awesometree.mobile.data.ConnectionStore
import dev.awesometree.mobile.data.ProjectDetail
import dev.awesometree.mobile.data.ProjectInfo
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ProjectsScreen(connectionStore: ConnectionStore) {
    val connection = connectionStore.connection.collectAsState().value ?: return
    val client = remember(connection) { ApiClient(connection) }
    val scope = rememberCoroutineScope()

    var projects by remember { mutableStateOf<List<ProjectInfo>>(emptyList()) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var showCreate by remember { mutableStateOf(false) }
    var editingProject by remember { mutableStateOf<ProjectInfo?>(null) }

    fun refresh() {
        scope.launch {
            loading = true
            error = null
            client.listProjects()
                .onSuccess { projects = it; loading = false }
                .onFailure { error = it.message; loading = false }
        }
    }

    LaunchedEffect(connection) { refresh() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Projects") },
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
                projects.isEmpty() -> {
                    Text(
                        "No projects",
                        modifier = Modifier.align(Alignment.Center),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                else -> {
                    LazyColumn(Modifier.fillMaxSize()) {
                        items(projects, key = { it.name }) { proj ->
                            ProjectItem(
                                proj,
                                onEdit = { editingProject = proj },
                                onDelete = {
                                    scope.launch {
                                        client.deleteProject(proj.name)
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
        ProjectFormDialog(
            title = "Create Project",
            initial = ProjectDetail(name = "", repo = null, branch = "master"),
            onDismiss = { showCreate = false },
            onSubmit = { detail ->
                scope.launch {
                    client.createProject(detail)
                        .onSuccess { showCreate = false; refresh() }
                        .onFailure { error = it.message }
                }
            },
        )
    }

    editingProject?.let { proj ->
        ProjectFormDialog(
            title = "Edit ${proj.name}",
            initial = ProjectDetail(
                name = proj.name,
                repo = proj.repo,
                branch = proj.branch,
            ),
            nameEditable = false,
            onDismiss = { editingProject = null },
            onSubmit = { detail ->
                scope.launch {
                    client.updateProject(proj.name, detail)
                        .onSuccess { editingProject = null; refresh() }
                        .onFailure { error = it.message }
                }
            },
        )
    }
}

@Composable
private fun ProjectItem(
    proj: ProjectInfo,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    var showConfirm by remember { mutableStateOf(false) }

    ListItem(
        headlineContent = { Text(proj.name) },
        supportingContent = {
            val parts = listOfNotNull(
                proj.repo?.let { "repo: $it" },
                proj.branch?.let { "branch: $it" },
            )
            if (parts.isNotEmpty()) Text(parts.joinToString("  "))
        },
        trailingContent = {
            Row {
                IconButton(onClick = onEdit) {
                    Icon(Icons.Default.Edit, "Edit")
                }
                IconButton(onClick = { showConfirm = true }) {
                    Icon(Icons.Default.Delete, "Delete")
                }
            }
        },
    )

    if (showConfirm) {
        AlertDialog(
            onDismissRequest = { showConfirm = false },
            title = { Text("Delete project?") },
            text = { Text("Delete \"${proj.name}\"? This cannot be undone.") },
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
private fun ProjectFormDialog(
    title: String,
    initial: ProjectDetail,
    nameEditable: Boolean = true,
    onDismiss: () -> Unit,
    onSubmit: (ProjectDetail) -> Unit,
) {
    var name by remember { mutableStateOf(initial.name) }
    var repo by remember { mutableStateOf(initial.repo ?: "") }
    var branch by remember { mutableStateOf(initial.branch ?: "master") }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Name") },
                    singleLine = true,
                    enabled = nameEditable,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(
                    value = repo,
                    onValueChange = { repo = it },
                    label = { Text("Repository path") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(8.dp))
                OutlinedTextField(
                    value = branch,
                    onValueChange = { branch = it },
                    label = { Text("Branch") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        },
        confirmButton = {
            TextButton(
                enabled = name.isNotBlank(),
                onClick = {
                    onSubmit(
                        ProjectDetail(
                            name = name.trim(),
                            repo = repo.trim().ifEmpty { null },
                            branch = branch.trim().ifEmpty { null },
                        )
                    )
                },
            ) { Text("Save") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}
