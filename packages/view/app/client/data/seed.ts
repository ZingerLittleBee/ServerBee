import fs from "fs"
import path from "path"
import { faker } from "@faker-js/faker"

import { labels, priorities, statuses } from "./data"

const tasks = Array.from({ length: 100 }, () => ({
  id: faker.string.nanoid(),
  title: faker.hacker.phrase().replace(/^./, (letter) => letter.toUpperCase()),
  status: faker.helpers.arrayElement(statuses).value,
  label: faker.helpers.arrayElement(labels).value,
  priority: faker.helpers.arrayElement(priorities).value,
  lct: faker.date.past(),
}))

fs.writeFileSync(
  path.join(__dirname, "tasks.json"),
  JSON.stringify(tasks, null, 2)
)

console.log("✅ Tasks data generated.")
