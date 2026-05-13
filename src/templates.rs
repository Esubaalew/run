/// A built-in source snippet template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    /// Canonical language id.
    pub language: &'static str,
    /// Template name used on the CLI.
    pub name: &'static str,
    /// Template source code.
    pub source: &'static str,
}

/// Built-in snippet templates.
pub const TEMPLATES: &[Template] = &[
    Template {
        language: "python",
        name: "http-server",
        source: "from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler\n\nif __name__ == \"__main__\":\n    server = ThreadingHTTPServer((\"127.0.0.1\", 8000), SimpleHTTPRequestHandler)\n    print(\"serving on http://127.0.0.1:8000\")\n    server.serve_forever()\n",
    },
    Template {
        language: "python",
        name: "argparse-cli",
        source: "import argparse\n\nparser = argparse.ArgumentParser()\nparser.add_argument(\"name\")\nargs = parser.parse_args()\nprint(f\"hello, {args.name}\")\n",
    },
    Template {
        language: "python",
        name: "pytest-skeleton",
        source: "def add(a, b):\n    return a + b\n\n\ndef test_add():\n    assert add(2, 3) == 5\n",
    },
    Template {
        language: "javascript",
        name: "fetch-json",
        source: "const res = await fetch(process.argv[2]);\nif (!res.ok) throw new Error(`${res.status} ${res.statusText}`);\nconsole.log(JSON.stringify(await res.json(), null, 2));\n",
    },
    Template {
        language: "javascript",
        name: "express-app",
        source: "import express from \"express\";\n\nconst app = express();\napp.get(\"/\", (_req, res) => res.send(\"hello\"));\napp.listen(3000, () => console.log(\"http://127.0.0.1:3000\"));\n",
    },
    Template {
        language: "javascript",
        name: "node-cli",
        source: "#!/usr/bin/env node\nconst [name = \"world\"] = process.argv.slice(2);\nconsole.log(`hello, ${name}`);\n",
    },
    Template {
        language: "typescript",
        name: "fetch-json",
        source: "const url = Deno.args[0];\nconst res = await fetch(url);\nif (!res.ok) throw new Error(`${res.status} ${res.statusText}`);\nconsole.log(JSON.stringify(await res.json(), null, 2));\n",
    },
    Template {
        language: "typescript",
        name: "express-app",
        source: "import express from \"npm:express\";\n\nconst app = express();\napp.get(\"/\", (_req, res) => res.send(\"hello\"));\napp.listen(3000, () => console.log(\"http://127.0.0.1:3000\"));\n",
    },
    Template {
        language: "typescript",
        name: "zod-validator",
        source: "import { z } from \"npm:zod\";\n\nconst User = z.object({ id: z.number(), name: z.string() });\nconsole.log(User.parse({ id: 1, name: \"Ada\" }));\n",
    },
    Template {
        language: "rust",
        name: "async-main",
        source: "use anyhow::Result;\n\n#[tokio::main]\nasync fn main() -> Result<()> {\n    println!(\"hello async\");\n    Ok(())\n}\n",
    },
    Template {
        language: "rust",
        name: "cli-clap",
        source: "use clap::Parser;\n\n#[derive(Parser)]\nstruct Args { name: String }\n\nfn main() {\n    let args = Args::parse();\n    println!(\"hello, {}\", args.name);\n}\n",
    },
    Template {
        language: "rust",
        name: "serde-roundtrip",
        source: "use serde::{Deserialize, Serialize};\n\n#[derive(Debug, Serialize, Deserialize)]\nstruct Item { id: u64, name: String }\n\nfn main() {\n    let item = Item { id: 1, name: \"demo\".into() };\n    println!(\"{}\", serde_json::to_string_pretty(&item).unwrap());\n}\n",
    },
    Template {
        language: "go",
        name: "goroutine-pool",
        source: "package main\n\nimport \"fmt\"\n\nfunc main() {\n    jobs := make(chan int)\n    for w := 0; w < 4; w++ {\n        go func(id int) { for job := range jobs { fmt.Println(id, job) } }(w)\n    }\n    for i := 0; i < 10; i++ { jobs <- i }\n    close(jobs)\n}\n",
    },
    Template {
        language: "go",
        name: "http-server",
        source: "package main\n\nimport (\n    \"fmt\"\n    \"net/http\"\n)\n\nfunc main() {\n    http.HandleFunc(\"/\", func(w http.ResponseWriter, r *http.Request) { fmt.Fprintln(w, \"hello\") })\n    http.ListenAndServe(\":8080\", nil)\n}\n",
    },
    Template {
        language: "go",
        name: "errgroup",
        source: "package main\n\nimport (\n    \"context\"\n    \"fmt\"\n    \"golang.org/x/sync/errgroup\"\n)\n\nfunc main() {\n    g, _ := errgroup.WithContext(context.Background())\n    g.Go(func() error { fmt.Println(\"work\"); return nil })\n    if err := g.Wait(); err != nil { panic(err) }\n}\n",
    },
    Template {
        language: "c",
        name: "linked-list",
        source: "#include <stdio.h>\n#include <stdlib.h>\n\ntypedef struct Node { int value; struct Node *next; } Node;\n\nint main(void) {\n    Node *head = malloc(sizeof(Node));\n    head->value = 1; head->next = NULL;\n    printf(\"%d\\n\", head->value);\n    free(head);\n    return 0;\n}\n",
    },
    Template {
        language: "c",
        name: "getline-loop",
        source: "#include <stdio.h>\n#include <stdlib.h>\n\nint main(void) {\n    char *line = NULL;\n    size_t cap = 0;\n    while (getline(&line, &cap, stdin) != -1) printf(\"%s\", line);\n    free(line);\n    return 0;\n}\n",
    },
    Template {
        language: "c",
        name: "pthread-counter",
        source: "#include <pthread.h>\n#include <stdio.h>\n\nvoid *worker(void *arg) { printf(\"worker %ld\\n\", (long)arg); return NULL; }\nint main(void) { pthread_t t; pthread_create(&t, NULL, worker, (void *)1); pthread_join(t, NULL); }\n",
    },
    Template {
        language: "cpp",
        name: "vector-sort",
        source: "#include <algorithm>\n#include <iostream>\n#include <vector>\n\nint main() {\n    std::vector<int> xs{3, 1, 2};\n    std::sort(xs.begin(), xs.end());\n    for (int x : xs) std::cout << x << '\\n';\n}\n",
    },
    Template {
        language: "cpp",
        name: "thread-pool",
        source: "#include <future>\n#include <iostream>\n#include <vector>\n\nint main() {\n    std::vector<std::future<int>> jobs;\n    for (int i = 0; i < 4; ++i) jobs.push_back(std::async([i] { return i * i; }));\n    for (auto &job : jobs) std::cout << job.get() << '\\n';\n}\n",
    },
    Template {
        language: "cpp",
        name: "lambda-async",
        source: "#include <future>\n#include <iostream>\n\nint main() {\n    auto result = std::async([] { return 42; });\n    std::cout << result.get() << '\\n';\n}\n",
    },
    Template {
        language: "java",
        name: "http-server",
        source: "import com.sun.net.httpserver.HttpServer;\nimport java.net.InetSocketAddress;\n\npublic class Main {\n  public static void main(String[] args) throws Exception {\n    var server = HttpServer.create(new InetSocketAddress(8080), 0);\n    server.createContext(\"/\", ex -> { byte[] b = \"hello\".getBytes(); ex.sendResponseHeaders(200, b.length); ex.getResponseBody().write(b); ex.close(); });\n    server.start();\n  }\n}\n",
    },
    Template {
        language: "java",
        name: "executor-pool",
        source: "import java.util.concurrent.*;\n\npublic class Main {\n  public static void main(String[] args) throws Exception {\n    var pool = Executors.newFixedThreadPool(4);\n    var f = pool.submit(() -> 42);\n    System.out.println(f.get());\n    pool.shutdown();\n  }\n}\n",
    },
    Template {
        language: "java",
        name: "record-json",
        source: "record User(long id, String name) {}\n\npublic class Main {\n  public static void main(String[] args) {\n    var user = new User(1, \"Ada\");\n    System.out.println(user);\n  }\n}\n",
    },
    Template {
        language: "ruby",
        name: "http-server",
        source: "require \"webrick\"\n\nserver = WEBrick::HTTPServer.new(Port: 8000)\nserver.mount_proc(\"/\") { |_req, res| res.body = \"hello\\n\" }\ntrap(\"INT\") { server.shutdown }\nserver.start\n",
    },
    Template {
        language: "ruby",
        name: "bundler-script",
        source: "require \"bundler/inline\"\n\ngemfile do\n  source \"https://rubygems.org\"\nend\n\nputs \"ready\"\n",
    },
    Template {
        language: "ruby",
        name: "rspec-skeleton",
        source: "def add(a, b) = a + b\n\nRSpec.describe \"add\" do\n  it { expect(add(2, 3)).to eq(5) }\nend\n",
    },
    Template {
        language: "bash",
        name: "arg-parser",
        source: "#!/usr/bin/env bash\nset -euo pipefail\n\nname=\"world\"\nwhile [[ $# -gt 0 ]]; do\n  case \"$1\" in\n    --name) name=\"$2\"; shift 2 ;;\n    *) echo \"unknown arg: $1\" >&2; exit 2 ;;\n  esac\ndone\nprintf 'hello, %s\\n' \"$name\"\n",
    },
    Template {
        language: "bash",
        name: "safe-script",
        source: "#!/usr/bin/env bash\nset -euo pipefail\nIFS=$'\\n\\t'\n\nmain() {\n  echo \"safe shell\"\n}\nmain \"$@\"\n",
    },
    Template {
        language: "bash",
        name: "parallel-xargs",
        source: "#!/usr/bin/env bash\nset -euo pipefail\n\nprintf '%s\\n' \"$@\" | xargs -n1 -P4 -I{} sh -c 'echo processing {}'\n",
    },
];

/// Return a template by language and name.
pub fn find(language: &str, name: &str) -> Option<&'static Template> {
    TEMPLATES
        .iter()
        .find(|template| template.language == language && template.name == name)
}

/// Return all template names for a language.
pub fn names_for_language(language: &str) -> Vec<&'static str> {
    let mut names = TEMPLATES
        .iter()
        .filter_map(|template| (template.language == language).then_some(template.name))
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}
