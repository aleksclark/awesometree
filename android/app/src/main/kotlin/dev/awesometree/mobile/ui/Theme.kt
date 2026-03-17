package dev.awesometree.mobile.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val CatppuccinMocha = darkColorScheme(
    primary = Color(0xFF89B4FA),
    onPrimary = Color(0xFF1E1E2E),
    primaryContainer = Color(0xFF45475A),
    onPrimaryContainer = Color(0xFFCDD6F4),
    secondary = Color(0xFFA6E3A1),
    onSecondary = Color(0xFF1E1E2E),
    background = Color(0xFF1E1E2E),
    onBackground = Color(0xFFCDD6F4),
    surface = Color(0xFF1E1E2E),
    onSurface = Color(0xFFCDD6F4),
    surfaceVariant = Color(0xFF313244),
    onSurfaceVariant = Color(0xFF6C7086),
    error = Color(0xFFF38BA8),
    onError = Color(0xFF1E1E2E),
    outline = Color(0xFF313244),
)

@Composable
fun AwesometreeTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = CatppuccinMocha,
        content = content,
    )
}
