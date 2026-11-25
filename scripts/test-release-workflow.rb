require "yaml"

def check(condition, message)
  abort message unless condition
end

def step(job, name)
  job.fetch("steps").find { |entry| entry["name"] == name } ||
    abort("Missing workflow step: #{name}")
end

workflow = YAML.safe_load(File.read(File.join(__dir__, "../.github/workflows/release-apps.yml")))
jobs = workflow.fetch("jobs")
publish_only = "needs.metadata.outputs.should_upload == 'true'"

check(workflow.dig("permissions", "contents") == "read", "Keep default contents access read-only")
writers = jobs.select { |_, job| job.dig("permissions", "contents") == "write" }.keys
check(writers == ["publish-release-assets"], "Only the publication job may write release assets")
prepare = jobs.fetch("prepare-release-assets")
publish = jobs.fetch("publish-release-assets")
check(prepare.dig("permissions", "contents") == "read", "Generate metadata with read-only access")
check(step(prepare, "Checkout release ref").dig("with", "persist-credentials") == false,
      "Do not retain checkout credentials when generating metadata")
check(publish["if"] == publish_only, "Run the publication job only for a public release")
check(step(publish, "Checkout release commit").dig("with", "persist-credentials") == false,
      "Do not retain checkout credentials in the publication job")
artifact = step(prepare, "Upload verified release files").dig("with", "name")
check(step(publish, "Download verified release files").dig("with", "name") == artifact,
      "Publish the artifacts verified by the preparation job")

macos = jobs.fetch("build-macos")
check(macos.fetch("env", {}).values.none? { |value| value.to_s.include?("secrets.BMZ_MACOS_") },
      "Do not expose macOS secrets to an entire build job")
secret_steps = macos.fetch("steps").select do |entry|
  entry.fetch("env", {}).values.any? { |value| value.to_s.include?("secrets.BMZ_MACOS_") }
end
check(secret_steps.map { |entry| entry["name"] } ==
        ["Import macOS signing certificate", "Package macOS app", "Notarize macOS app"],
      "Scope macOS secrets to the signing, packaging, and notarization steps")
["Import macOS signing certificate", "Notarize macOS app"].each do |name|
  check(step(macos, name)["if"] == publish_only, "Run #{name} only for a public release")
end
identity = step(macos, "Package macOS app").dig("env", "BMZ_MACOS_CODESIGN_IDENTITY")
check(identity == "${{ #{publish_only} && secrets.BMZ_MACOS_CODESIGN_IDENTITY || '' }}",
      "Keep the signing identity out of non-publishing packaging runs")

puts "Release workflow permissions and macOS secret scope verified"
