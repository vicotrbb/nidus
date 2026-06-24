pub(crate) fn docs_html(title: &str, spec_url: &str) -> String {
    let title = escape_html(title);
    let spec_url = escape_js_string(spec_url);
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>{title} Documentation</title>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/swagger-ui-dist/swagger-ui.css">
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://cdn.jsdelivr.net/npm/swagger-ui-dist/swagger-ui-bundle.js"></script>
  <script>
    window.ui = SwaggerUIBundle({{
      url: "{spec_url}",
      dom_id: "#swagger-ui"
    }});
  </script>
</body>
</html>"##
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn escape_js_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
