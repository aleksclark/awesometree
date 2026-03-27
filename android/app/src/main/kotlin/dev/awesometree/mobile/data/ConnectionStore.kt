package dev.awesometree.mobile.data

import android.content.Context
import android.content.SharedPreferences
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

data class ServerConnection(
    val host: String,
    val port: Int,
    val token: String,
    val useHttps: Boolean = false,
) {
    val scheme: String get() = if (useHttps) "https" else "http"
    val baseUrl: String get() = "$scheme://$host:$port"
}

class ConnectionStore(context: Context) {
    private val prefs: SharedPreferences =
        context.getSharedPreferences("awesometree_connection", Context.MODE_PRIVATE)

    private val _connection = MutableStateFlow(load())
    val connection: StateFlow<ServerConnection?> = _connection.asStateFlow()

    private fun load(): ServerConnection? {
        val host = prefs.getString("host", null) ?: return null
        val port = prefs.getInt("port", 0)
        val token = prefs.getString("token", null) ?: return null
        val useHttps = prefs.getBoolean("useHttps", false)
        if (port == 0) return null
        return ServerConnection(host, port, token, useHttps)
    }

    fun save(conn: ServerConnection) {
        prefs.edit()
            .putString("host", conn.host)
            .putInt("port", conn.port)
            .putString("token", conn.token)
            .putBoolean("useHttps", conn.useHttps)
            .apply()
        _connection.value = conn
    }

    fun clear() {
        prefs.edit().clear().apply()
        _connection.value = null
    }

    val isConnected: Boolean get() = _connection.value != null
}
