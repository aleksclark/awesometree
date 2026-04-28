package main

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"time"
)

var agentCard = map[string]interface{}{
	"name":        "echo-agent",
	"description": "Test echo agent for ARP compliance testing",
	"version":     "1.0.0",
	"supportedInterfaces": []map[string]interface{}{
		{
			"url":             agentURL(),
			"protocolBinding": "HTTP+JSON",
			"protocolVersion": "1.0",
		},
	},
	"capabilities": map[string]interface{}{
		"streaming": true,
	},
	"skills": []map[string]interface{}{
		{
			"id":          "echo",
			"name":        "Echo",
			"description": "Echoes messages",
			"tags":        []string{"echo"},
		},
	},
	"defaultInputModes":  []string{"text/plain"},
	"defaultOutputModes": []string{"text/plain"},
}

func agentURL() string {
	if u := os.Getenv("ECHO_AGENT_URL"); u != "" {
		return u
	}
	return "http://echo-agent:9200"
}

func listenAddr() string {
	if a := os.Getenv("ECHO_AGENT_ADDR"); a != "" {
		return a
	}
	return ":9200"
}

func handleAgentCard(w http.ResponseWriter, r *http.Request) {
	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(agentCard)
}

func handleSendMessage(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req map[string]interface{}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "bad request: "+err.Error(), http.StatusBadRequest)
		return
	}

	inputText := extractText(req)
	taskID := fmt.Sprintf("echo-task-%d", time.Now().UnixNano())

	resp := map[string]interface{}{
		"id":        taskID,
		"status":    "completed",
		"artifacts": []map[string]interface{}{
			{
				"parts": []map[string]interface{}{
					{
						"text_part": map[string]interface{}{
							"text": "echo: " + inputText,
						},
					},
				},
			},
		},
	}

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(resp)
}

func handleStreamMessage(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req map[string]interface{}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, "bad request: "+err.Error(), http.StatusBadRequest)
		return
	}

	inputText := extractText(req)
	taskID := fmt.Sprintf("echo-task-%d", time.Now().UnixNano())

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")

	flusher, ok := w.(http.Flusher)
	if !ok {
		http.Error(w, "streaming not supported", http.StatusInternalServerError)
		return
	}

	// Send status update
	statusEvent := map[string]interface{}{
		"id":     taskID,
		"status": "working",
	}
	data, _ := json.Marshal(statusEvent)
	fmt.Fprintf(w, "event: status\ndata: %s\n\n", data)
	flusher.Flush()

	// Send artifact with echoed text
	artifactEvent := map[string]interface{}{
		"id":     taskID,
		"status": "completed",
		"artifacts": []map[string]interface{}{
			{
				"parts": []map[string]interface{}{
					{
						"text_part": map[string]interface{}{
							"text": "echo: " + inputText,
						},
					},
				},
			},
		},
	}
	data, _ = json.Marshal(artifactEvent)
	fmt.Fprintf(w, "event: status\ndata: %s\n\n", data)
	flusher.Flush()
}

func extractText(req map[string]interface{}) string {
	msg, ok := req["message"].(map[string]interface{})
	if !ok {
		return "(no message)"
	}
	parts, ok := msg["parts"].([]interface{})
	if !ok || len(parts) == 0 {
		return "(no parts)"
	}
	part, ok := parts[0].(map[string]interface{})
	if !ok {
		return "(invalid part)"
	}
	tp, ok := part["text_part"].(map[string]interface{})
	if !ok {
		return "(no text_part)"
	}
	text, ok := tp["text"].(string)
	if !ok {
		return "(no text)"
	}
	return text
}

func main() {
	mux := http.NewServeMux()
	mux.HandleFunc("/.well-known/agent-card.json", handleAgentCard)
	mux.HandleFunc("/message:send", handleSendMessage)
	mux.HandleFunc("/message:stream", handleStreamMessage)

	addr := listenAddr()
	log.Printf("echo-agent listening on %s", addr)
	if err := http.ListenAndServe(addr, mux); err != nil {
		log.Fatal(err)
	}
}
