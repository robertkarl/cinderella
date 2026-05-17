
llama=/opt/homebrew/bin/llama-server
modelcachedir=/Users/robertkarl/Library/Application Support/Glass Slipper/Models
qwen36=Qwen3.6-35B-A3B-UD-Q5_K_M.gguf
9boblit=mradermacher_Huihui-Qwen3.5-9B-abliterated-GGUF_Huihui-Qwen3.5-9B-abliterated.Q4_K_M.gguf

startllama_9boblit_ctx16k:
	${llama} --model  "${modelcachedir}/${9boblit}" --host localhost --port 8081 --ctx-size 16384 --jinja 

startllama_qwen36:
	${llama} --model  "${modelcachedir}/${qwen36}" --host localhost --port 8081 --ctx-size 200000 --jinja --flash-attn on --cache-type-k q8_0 --cache-type-v q8_0

startllama_9boblit_32k:
	${llama} --model  "${modelcachedir}/${9boblit}" --host localhost --port 8081 --ctx-size 32768 --jinja 

stop_all_llama_server:
	killall llama-server

edit_pi_configs:
	vim /Users/robertkarl/.pi/agent/models.json /Users/robertkarl/.pi/agent/settings.json -p

pi-read-only:
	pi --tools read,grep,find,ls

# Run glass-slipper against an already-running llama-server on port 8081 (no server startup)
cindy-no-start-llama:
	cargo run -- --api-url http://localhost:8081 .

# Run glass-slipper against the homelab 35B MoE (llama-swap on VM 220)
cindy-remote:
	cargo run -- --api-url http://192.168.50.4:11434 --model-name "qwen3.5:35b-a3b-coding" .

remote-query-models:
	curl http://192.168.50.4:11434/v1/models

test:
	cargo test

deploy:
	@echo "No deploy target configured. Glass Slipper is a local CLI tool."

smoke-test:
	@echo "No smoke test configured. Glass Slipper is a local CLI tool."

build_dmg:
	./scripts/package-macos.sh

open_from_dmg:
	hdiutil detach /Volumes/Glass\ Slipper 2>/dev/null || true
	hdiutil attach build/Glass\ Slipper.dmg
	open /Volumes/Glass\ Slipper/Glass\ Slipper.app

.PHONY: stop_all_llama_server startllama pidev cindy-no-start-llama cindy-remote remote-query-models test deploy smoke-test build_dmg open_from_dmg
