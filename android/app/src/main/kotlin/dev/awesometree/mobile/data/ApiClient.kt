package dev.awesometree.mobile.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URL

class ApiClient(private val connection: ServerConnection) {

    private suspend fun request(
        method: String,
        path: String,
        body: String? = null,
    ): Result<String> = withContext(Dispatchers.IO) {
        try {
            val url = URL("http://${connection.host}:${connection.port}$path")
            val conn = (url.openConnection() as HttpURLConnection).apply {
                requestMethod = method
                setRequestProperty("Authorization", "Bearer ${connection.token}")
                setRequestProperty("Content-Type", "application/json")
                connectTimeout = 10_000
                readTimeout = 30_000
                if (body != null) {
                    doOutput = true
                    outputStream.bufferedWriter().use { it.write(body) }
                }
            }

            val code = conn.responseCode
            val stream = if (code < 400) conn.inputStream else conn.errorStream
            val text = stream?.let {
                BufferedReader(InputStreamReader(it)).use { r -> r.readText() }
            } ?: ""

            if (code in 200..299) {
                Result.success(text)
            } else {
                Result.failure(ApiException(code, text))
            }
        } catch (e: Exception) {
            Result.failure(e)
        }
    }

    suspend fun listWorkspaces(): Result<List<WorkspaceInfo>> =
        request("GET", "/api/workspaces").map { parseWorkspaceList(it) }

    suspend fun getWorkspace(name: String): Result<WorkspaceInfo> =
        request("GET", "/api/workspaces/$name").map { parseWorkspace(it) }

    suspend fun createWorkspace(name: String, project: String): Result<WorkspaceInfo> {
        val body = JSONObject().put("name", name).put("project", project).toString()
        return request("POST", "/api/workspaces", body).map { parseWorkspace(it) }
    }

    suspend fun deleteWorkspace(name: String): Result<Unit> =
        request("DELETE", "/api/workspaces/$name").map { }

    suspend fun listProjects(): Result<List<ProjectInfo>> =
        request("GET", "/api/projects").map { parseProjectList(it) }

    suspend fun getProject(name: String): Result<ProjectDetail> =
        request("GET", "/api/projects/$name").map { parseProjectDetail(it) }

    suspend fun createProject(project: ProjectDetail): Result<ProjectDetail> =
        request("POST", "/api/projects", project.toJson()).map { parseProjectDetail(it) }

    suspend fun updateProject(name: String, project: ProjectDetail): Result<ProjectDetail> =
        request("PUT", "/api/projects/$name", project.toJson()).map { parseProjectDetail(it) }

    suspend fun deleteProject(name: String): Result<Unit> =
        request("DELETE", "/api/projects/$name").map { }

    suspend fun acpSend(workspace: String, message: String): Result<String> =
        request("POST", "/acp/$workspace", message)
}

class ApiException(val code: Int, message: String) : Exception("HTTP $code: $message")

data class WorkspaceInfo(
    val name: String,
    val project: String,
    val active: Boolean,
    val tagIndex: Int,
    val dir: String,
    val acpPort: Int?,
)

data class ProjectInfo(
    val name: String,
    val repo: String?,
    val branch: String?,
)

data class ProjectDetail(
    val version: String = "1",
    val name: String,
    val repo: String? = null,
    val branch: String? = null,
) {
    fun toJson(): String = JSONObject().apply {
        put("version", version)
        put("name", name)
        repo?.let { put("repo", it) }
        branch?.let { put("branch", it) }
    }.toString()
}

private fun parseWorkspace(json: String): WorkspaceInfo {
    val obj = JSONObject(json)
    return WorkspaceInfo(
        name = obj.getString("name"),
        project = obj.getString("project"),
        active = obj.getBoolean("active"),
        tagIndex = obj.getInt("tag_index"),
        dir = obj.getString("dir"),
        acpPort = if (obj.has("acp_port") && !obj.isNull("acp_port")) obj.getInt("acp_port") else null,
    )
}

private fun parseWorkspaceList(json: String): List<WorkspaceInfo> {
    val arr = JSONArray(json)
    return (0 until arr.length()).map { parseWorkspace(arr.getJSONObject(it).toString()) }
}

private fun parseProjectList(json: String): List<ProjectInfo> {
    val arr = JSONArray(json)
    return (0 until arr.length()).map { obj ->
        val o = arr.getJSONObject(obj)
        ProjectInfo(
            name = o.getString("name"),
            repo = if (o.has("repo") && !o.isNull("repo")) o.getString("repo") else null,
            branch = if (o.has("branch") && !o.isNull("branch")) o.getString("branch") else null,
        )
    }
}

private fun parseProjectDetail(json: String): ProjectDetail {
    val obj = JSONObject(json)
    return ProjectDetail(
        version = obj.optString("version", "1"),
        name = obj.getString("name"),
        repo = if (obj.has("repo") && !obj.isNull("repo")) obj.getString("repo") else null,
        branch = if (obj.has("branch") && !obj.isNull("branch")) obj.getString("branch") else null,
    )
}
