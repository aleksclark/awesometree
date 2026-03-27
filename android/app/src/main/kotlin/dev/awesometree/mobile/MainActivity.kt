package dev.awesometree.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.filled.FolderOpen
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Workspaces
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.navigation.NavController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import dev.awesometree.mobile.data.ConnectionStore
import dev.awesometree.mobile.ui.AwesometreeTheme
import dev.awesometree.mobile.ui.acp.AcpScreen
import dev.awesometree.mobile.ui.projects.ProjectsScreen
import dev.awesometree.mobile.ui.settings.SettingsScreen
import dev.awesometree.mobile.ui.workspaces.WorkspacesScreen

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        val connectionStore = ConnectionStore(applicationContext)

        setContent {
            AwesometreeTheme {
                val connection by connectionStore.connection.collectAsState()

                if (connection == null) {
                    SettingsScreen(
                        connectionStore = connectionStore,
                        fullScreen = true,
                    )
                } else {
                    MainScaffold(connectionStore)
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MainScaffold(connectionStore: ConnectionStore) {
    val navController = rememberNavController()
    val currentRoute = navController.currentBackStackEntryAsState().value?.destination?.route

    data class NavItem(val route: String, val label: String, val icon: @Composable () -> Unit)

    val items = listOf(
        NavItem("workspaces", "Workspaces") { Icon(Icons.Default.Workspaces, null) },
        NavItem("projects", "Projects") { Icon(Icons.Default.FolderOpen, null) },
        NavItem("acp", "Agent") { Icon(Icons.AutoMirrored.Filled.Chat, null) },
        NavItem("settings", "Settings") { Icon(Icons.Default.Settings, null) },
    )

    Scaffold(
        bottomBar = {
            NavigationBar {
                items.forEach { item ->
                    NavigationBarItem(
                        selected = currentRoute == item.route,
                        onClick = {
                            if (currentRoute != item.route) {
                                navController.navigate(item.route) {
                                    popUpTo("workspaces") { saveState = true }
                                    launchSingleTop = true
                                    restoreState = true
                                }
                            }
                        },
                        icon = item.icon,
                        label = { Text(item.label) },
                    )
                }
            }
        },
    ) { padding ->
        NavHost(
            navController = navController,
            startDestination = "workspaces",
            modifier = Modifier.padding(padding),
        ) {
            composable("workspaces") { WorkspacesScreen(connectionStore, navController) }
            composable("projects") { ProjectsScreen(connectionStore) }
            composable("acp") { AcpScreen(connectionStore) }
            composable("acp/{workspace}") { backStackEntry ->
                val workspace = backStackEntry.arguments?.getString("workspace")
                AcpScreen(connectionStore, preselectedWorkspace = workspace)
            }
            composable("settings") { SettingsScreen(connectionStore) }
        }
    }
}
