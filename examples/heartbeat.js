// Tiny test job for Crontab.
// Appends a timestamped line to heartbeat.log next to this script,
// and prints the same line to stdout so it lands in the per-job log.
//
// Use in a job's command:
//   node "C:\Users\hp\Documents\claude\cron\examples\heartbeat.js"
//
// Try schedule:  * * * * *   (every minute)

const fs = require("fs");
const path = require("path");

const stamp = new Date().toISOString();
const line = `[${stamp}] heartbeat ok pid=${process.pid}\n`;

fs.appendFileSync(path.join(__dirname, "heartbeat.log"), line);
process.stdout.write(line);
