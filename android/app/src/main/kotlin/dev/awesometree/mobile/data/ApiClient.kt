package dev.awesometree.mobile.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

class ApiClient(private val connection: ServerConnection) {

    private suspend fun request(
        method: String,
        path: String,
        body: String? = null,
    ): Result<String> = withContext(Dispatchers.IO) {
        try {
            val url = URL("${connection.baseUrl}$path")
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

    suspend fun listWorkSessions(): Result<List<WorkSessionInfo>> =
        request("GET", "/api/work-sessions").map { parseWorkSessionList(it) }

    suspend fun getWorkSession(id: String): Result<WorkSessionInfo> =
        request("GET", "/api/work-sessions/${enc(id)}").map { parseWorkSessionView(JSONObject(it)) }

    suspend fun createWorkSession(
        workSessionId: String,
        projectId: String,
        workProfileId: String? = null,
        headless: Boolean = false,
    ): Result<WorkSessionInfo> {
        val body = JSONObject().apply {
            put("work_session_id", workSessionId)
            put("project_id", projectId)
            if (!workProfileId.isNullOrBlank()) put("work_profile_id", workProfileId)
            put("headless", headless)
        }.toString()
        return request("POST", "/api/work-sessions", body).map { json ->
            val obj = JSONObject(json)
            val ws = obj.optJSONObject("work_session") ?: obj
            val runtime = obj.optJSONObject("runtime")
            parseWorkSession(ws, runtime)
        }
    }

    suspend fun deleteWorkSession(id: String): Result<Unit> =
        request("DELETE", "/api/work-sessions/${enc(id)}").map { }

    suspend fun transitionWorkSession(id: String, state: String): Result<WorkSessionInfo> {
        val body = JSONObject().put("state", state).toString()
        return request("POST", "/api/work-sessions/${enc(id)}/transition", body)
            .map { parseWorkSessionView(JSONObject(it)) }
    }

    suspend fun listWorkProfiles(): Result<List<WorkProfileInfo>> =
        request("GET", "/api/work-profiles").map { parseWorkProfileList(it) }

    suspend fun listProjects(): Result<List<ProjectInfo>> =
        request("GET", "/api/projects").map { parseProjectList(it) }

    suspend fun getProject(id: String): Result<ProjectDetail> =
        request("GET", "/api/projects/${enc(id)}").map { parseProjectDetail(it) }

    suspend fun createProject(projectId: String, repo: String? = null, branch: String? = null): Result<ProjectInfo> {
        val body = JSONObject().apply {
            put("project_id", projectId)
            repo?.let { put("repo", it) }
            branch?.let { put("branch", it) }
        }.toString()
        return request("POST", "/api/projects", body).map { parseProjectInfo(JSONObject(it)) }
    }

    suspend fun deleteProject(id: String, expectedSourceRevision: String): Result<Unit> =
        request(
            "DELETE",
            "/api/projects/${enc(id)}?expected_source_revision=${enc(expectedSourceRevision)}",
        ).map { }
}

private fun enc(value: String): String = URLEncoder.encode(value, "UTF-8")

class ApiException(val code: Int, message: String) : Exception("HTTP $code: $message")

data class WorkSessionInfo(
    val workSessionId: String,
    val projectId: String?,
    val workProfileId: String?,
    val state: String,
    val displayName: String?,
    val dir: String?,
    val realizationStatus: String?,
    val headless: Boolean,
)

data class WorkProfileInfo(
    val workProfileId: String,
    val displayName: String?,
    val description: String?,
    val projectIds: List<String> = emptyList(),
) {
    fun appliesTo(projectId: String): Boolean =
        projectIds.isEmpty() || projectIds.contains(projectId)
}

data class ProjectInfo(
    val projectId: String,
    val title: String,
    val description: String?,
    val revision: String?,
    val sourceRevision: String?,
)

data class ProjectDetail(
    val projectId: String,
    val revision: String,
    val sourceRevision: String,
    val definitionJson: String,
)

private fun parseWorkSessionView(obj: JSONObject): WorkSessionInfo {
    val ws = obj.optJSONObject("work_session") ?: obj
    val runtime = obj.optJSONObject("runtime")
    return parseWorkSession(ws, runtime)
}

private fun parseWorkSession(ws: JSONObject, runtime: JSONObject?): WorkSessionInfo {
    val workspace = runtime?.optJSONObject("workspace")
    return WorkSessionInfo(
        workSessionId = ws.optString("work_session_id", ""),
        projectId = ws.optString("project_id", null),
        workProfileId = ws.optString("work_profile_id", null),
        state = when (val st = ws.opt("state")) {
            is String -> st
            else -> st?.toString()?.trim('"') ?: ""
        },
        displayName = ws.optString("display_name", null),
        dir = workspace?.optString("path", null),
        realizationStatus = runtime?.optString("realization_status", null),
        headless = runtime?.optBoolean("headless", false) ?: false,
    )
}

private fun parseWorkSessionList(json: String): List<WorkSessionInfo> {
    val arr = JSONArray(json)
    return (0 until arr.length()).map { parseWorkSessionView(arr.getJSONObject(it)) }
}

private fun parseWorkProfileList(json: String): List<WorkProfileInfo> {
    val arr = JSONArray(json)
    return (0 until arr.length()).map { i ->
        val o = arr.getJSONObject(i)
        val ids = o.optJSONArray("project_ids")
        val projectIds = if (ids == null) emptyList() else (0 until ids.length()).map { ids.getString(it) }
        WorkProfileInfo(
            workProfileId = o.optString("work_profile_id", ""),
            displayName = o.optString("display_name", null),
            description = o.optString("description", null),
            projectIds = projectIds,
        )
    }
}

private fun parseProjectInfo(o: JSONObject): ProjectInfo = ProjectInfo(
    projectId = o.optString("projectId", o.optString("project_id", "")),
    title = o.optString("title", ""),
    description = o.optString("description", null),
    revision = o.optString("revision", null),
    sourceRevision = o.optString("sourceRevision", o.optString("source_revision", null)),
)

private fun parseProjectList(json: String): List<ProjectInfo> {
    val arr = JSONArray(json)
    return (0 until arr.length()).map { parseProjectInfo(arr.getJSONObject(it)) }
}

private fun parseProjectDetail(json: String): ProjectDetail {
    val o = JSONObject(json)
    return ProjectDetail(
        projectId = o.optString("projectId", o.optString("project_id", "")),
        revision = o.optString("revision", ""),
        sourceRevision = o.optString("sourceRevision", o.optString("source_revision", "")),
        definitionJson = o.optJSONObject("definition")?.toString() ?: "{}",
    )
}
