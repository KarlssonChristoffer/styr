package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
)

// newMux holds the routes, so a test can call them without starting a server.
func newMux() *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]string{"status": "ok"})
	})
	return mux
}

func main() {
	addr := ":8080"
	if port := os.Getenv("PORT"); port != "" {
		addr = ":" + port
	}
	log.Printf("{{name}} listening on %s", addr)
	log.Fatal(http.ListenAndServe(addr, newMux()))
}
