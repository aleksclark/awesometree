// Resource-based project context proxy.
//
// This reference implementation reads .project.json files and serves
// project-scoped MCP endpoints at /mcp/{project}. Context is exposed
// via MCP resources using project:// URIs. Agent hosts that support
// resource auto-injection can automatically provide context to agents.
//
// Usage:
//
//	go run main.go [-addr :4568] [-config ~/.config/project-interop]
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"

	mcpsdk "github.com/modelcontextprotocol/go-sdk/mcp"
)

type ProjectDefinition struct {
	Version string          `json:"version"`
	Name    string          `json:"name"`
	Repo    string          `json:"repo,omitempty"`
	Branch  string          `json:"branch,omitempty"`
	Tools   json.RawMessage `json:"tools,omitempty"`
	Context *ContextConfig  `json:"context,omitempty"`
}

type ContextConfig struct {
	Files        []string `json:"files,omitempty"`
	RepoIncludes []string `json:"repoIncludes,omitempty"`
	MaxBytes     int      `json:"maxBytes,omitempty"`
}

type ContextEntry struct {
	Path     string `json:"path"`
	Source   string `json:"source"`
	MIMEType string `json:"mimeType"`
	Size     int    `json:"sizeBytes"`
}

type Proxy struct {
	configDir string
	logger    *slog.Logger
	mu        sync.RWMutex
	projects  map[string]*ProjectDefinition
	servers   map[string]*mcpsdk.Server
}

func NewProxy(configDir string, logger *slog.Logger) *Proxy {
	return &Proxy{
		configDir: configDir,
		logger:    logger,
		projects:  make(map[string]*ProjectDefinition),
		servers:   make(map[string]*mcpsdk.Server),
	}
}

func (p *Proxy) LoadProjects() error {
	dir := filepath.Join(p.configDir, "projects")
	entries, err := os.ReadDir(dir)
	if err != nil {
		return fmt.Errorf("reading projects dir: %w", err)
	}
	for _, e := range entries {
		if !strings.HasSuffix(e.Name(), ".project.json") {
			continue
		}
		data, err := os.ReadFile(filepath.Join(dir, e.Name()))
		if err != nil {
			p.logger.Warn("skipping project file", "file", e.Name(), "error", err)
			continue
		}
		var proj ProjectDefinition
		if err := json.Unmarshal(data, &proj); err != nil {
			p.logger.Warn("skipping invalid project", "file", e.Name(), "error", err)
			continue
		}
		if proj.Name == "" || proj.Version != "1" {
			p.logger.Warn("skipping project with invalid name/version", "file", e.Name())
			continue
		}
		p.mu.Lock()
		p.projects[proj.Name] = &proj
		p.mu.Unlock()
		p.logger.Info("loaded project", "name", proj.Name)
	}
	return nil
}

func (p *Proxy) getOrCreateServer(projectName string) (*mcpsdk.Server, error) {
	p.mu.RLock()
	srv, ok := p.servers[projectName]
	p.mu.RUnlock()
	if ok {
		return srv, nil
	}

	p.mu.RLock()
	proj, exists := p.projects[projectName]
	p.mu.RUnlock()
	if !exists {
		return nil, fmt.Errorf("project %q not found", projectName)
	}

	srv = mcpsdk.NewServer(
		&mcpsdk.Implementation{
			Name:    "project-interop-resource-proxy",
			Version: "0.1.0",
		},
		&mcpsdk.ServerOptions{
			Instructions: fmt.Sprintf("Project-scoped MCP server for %q. Read project:// resources to access project context.", projectName),
			Logger:       p.logger,
		},
	)

	p.registerResources(srv, proj)

	p.mu.Lock()
	p.servers[projectName] = srv
	p.mu.Unlock()
	return srv, nil
}

func (p *Proxy) registerResources(srv *mcpsdk.Server, proj *ProjectDefinition) {
	manifestURI := fmt.Sprintf("project://%s/context", proj.Name)
	srv.AddResource(
		&mcpsdk.Resource{
			URI:         manifestURI,
			Name:        "context-manifest",
			Description: fmt.Sprintf("Context manifest for %s: lists all available context files with metadata.", proj.Name),
			MIMEType:    "application/json",
		},
		p.makeManifestHandler(proj),
	)

	srv.AddResourceTemplate(
		&mcpsdk.ResourceTemplate{
			URITemplate: fmt.Sprintf("project://%s/context/{filePath}", proj.Name),
			Name:        "project-context-file",
			Description: fmt.Sprintf("A context file from the %s project.", proj.Name),
			MIMEType:    "text/plain",
		},
		p.makeFileHandler(proj),
	)

	entries := p.assembleManifest(proj)
	for _, entry := range entries {
		uri := fmt.Sprintf("project://%s/context/%s", proj.Name, entry.Path)
		e := entry
		srv.AddResource(
			&mcpsdk.Resource{
				URI:         uri,
				Name:        e.Path,
				Description: fmt.Sprintf("Context file: %s (source: %s)", e.Path, e.Source),
				MIMEType:    e.MIMEType,
				Size:        int64(e.Size),
			},
			p.makeSpecificFileHandler(proj, e.Path),
		)
	}
}

func (p *Proxy) makeManifestHandler(proj *ProjectDefinition) mcpsdk.ResourceHandler {
	return func(ctx context.Context, req *mcpsdk.ReadResourceRequest) (*mcpsdk.ReadResourceResult, error) {
		entries := p.assembleManifest(proj)
		data, err := json.MarshalIndent(entries, "", "  ")
		if err != nil {
			return nil, fmt.Errorf("marshaling manifest: %w", err)
		}
		return &mcpsdk.ReadResourceResult{
			Contents: []*mcpsdk.ResourceContents{{
				URI:      req.Params.URI,
				MIMEType: "application/json",
				Text:     string(data),
			}},
		}, nil
	}
}

func (p *Proxy) makeFileHandler(proj *ProjectDefinition) mcpsdk.ResourceHandler {
	return func(ctx context.Context, req *mcpsdk.ReadResourceRequest) (*mcpsdk.ReadResourceResult, error) {
		prefix := fmt.Sprintf("project://%s/context/", proj.Name)
		path := strings.TrimPrefix(req.Params.URI, prefix)
		if path == "" || path == req.Params.URI {
			return nil, fmt.Errorf("invalid resource URI: %s", req.Params.URI)
		}
		return p.readFile(proj, path, req.Params.URI)
	}
}

func (p *Proxy) makeSpecificFileHandler(proj *ProjectDefinition, path string) mcpsdk.ResourceHandler {
	return func(ctx context.Context, req *mcpsdk.ReadResourceRequest) (*mcpsdk.ReadResourceResult, error) {
		return p.readFile(proj, path, req.Params.URI)
	}
}

func (p *Proxy) readFile(proj *ProjectDefinition, path, uri string) (*mcpsdk.ReadResourceResult, error) {
	repoRoot := expandHome(proj.Repo)
	contextDir := filepath.Join(p.configDir, "context", proj.Name)

	candidates := []string{
		filepath.Join(contextDir, path),
		filepath.Join(repoRoot, path),
	}

	for _, candidate := range candidates {
		data, err := os.ReadFile(candidate)
		if err == nil {
			return &mcpsdk.ReadResourceResult{
				Contents: []*mcpsdk.ResourceContents{{
					URI:      uri,
					MIMEType: guessMIME(path),
					Text:     string(data),
				}},
			}, nil
		}
	}

	return nil, fmt.Errorf("context file not found: %s", path)
}

func (p *Proxy) assembleManifest(proj *ProjectDefinition) []ContextEntry {
	var entries []ContextEntry

	if proj.Context == nil {
		return entries
	}

	repoRoot := expandHome(proj.Repo)
	for _, inc := range proj.Context.RepoIncludes {
		full := filepath.Join(repoRoot, inc)
		matches, _ := filepath.Glob(full)
		if len(matches) == 0 {
			matches = []string{full}
		}
		for _, m := range matches {
			info, err := os.Stat(m)
			if err != nil {
				continue
			}
			rel, _ := filepath.Rel(repoRoot, m)
			entries = append(entries, ContextEntry{
				Path:     rel,
				Source:   "repo",
				MIMEType: guessMIME(rel),
				Size:     int(info.Size()),
			})
		}
	}

	contextDir := filepath.Join(p.configDir, "context", proj.Name)
	for _, f := range proj.Context.Files {
		full := filepath.Join(contextDir, f)
		info, err := os.Stat(full)
		if err != nil {
			continue
		}
		entries = append(entries, ContextEntry{
			Path:     f,
			Source:   "store",
			MIMEType: guessMIME(f),
			Size:     int(info.Size()),
		})
	}

	return deduplicate(entries)
}

func (p *Proxy) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/mcp/{project}", func(w http.ResponseWriter, r *http.Request) {
		projectName := r.PathValue("project")
		if projectName == "" {
			http.Error(w, "missing project name", http.StatusBadRequest)
			return
		}

		srv, err := p.getOrCreateServer(projectName)
		if err != nil {
			http.Error(w, err.Error(), http.StatusNotFound)
			return
		}

		handler := mcpsdk.NewStreamableHTTPHandler(
			func(r *http.Request) *mcpsdk.Server {
				return srv
			},
			&mcpsdk.StreamableHTTPOptions{
				Stateless: true,
				Logger:    p.logger,
			},
		)
		handler.ServeHTTP(w, r)
	})
	return mux
}

func deduplicate(entries []ContextEntry) []ContextEntry {
	seen := make(map[string]int)
	for i, e := range entries {
		seen[e.Path] = i
	}
	var result []ContextEntry
	added := make(map[string]bool)
	for _, e := range entries {
		idx := seen[e.Path]
		if !added[e.Path] {
			result = append(result, entries[idx])
			added[e.Path] = true
		}
	}
	return result
}

func expandHome(path string) string {
	if strings.HasPrefix(path, "~/") {
		home, _ := os.UserHomeDir()
		return filepath.Join(home, path[2:])
	}
	return path
}

func guessMIME(path string) string {
	switch strings.ToLower(filepath.Ext(path)) {
	case ".md":
		return "text/markdown"
	case ".txt":
		return "text/plain"
	case ".json":
		return "application/json"
	case ".yaml", ".yml":
		return "text/yaml"
	default:
		return "text/plain"
	}
}

func defaultConfigDir() string {
	if dir := os.Getenv("XDG_CONFIG_HOME"); dir != "" {
		return filepath.Join(dir, "project-interop")
	}
	home, _ := os.UserHomeDir()
	return filepath.Join(home, ".config", "project-interop")
}

func main() {
	addr := flag.String("addr", ":4568", "listen address")
	configDir := flag.String("config", defaultConfigDir(), "project-interop config directory")
	flag.Parse()

	logger := slog.Default()

	proxy := NewProxy(*configDir, logger)
	if err := proxy.LoadProjects(); err != nil {
		logger.Error("failed to load projects", "error", err)
		os.Exit(1)
	}

	logger.Info("starting resource-based project proxy", "addr", *addr)
	if err := http.ListenAndServe(*addr, proxy.Handler()); err != nil {
		logger.Error("server failed", "error", err)
		os.Exit(1)
	}
}
