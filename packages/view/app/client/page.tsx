import { promises as fs } from "fs"
import path from "path"
import * as React from "react"
import { Metadata } from "next"
import { z } from "zod"

import { columns } from "./components/columns"
import { DataTable } from "./components/data-table"
import { clientSchema } from "./data/schema"

export const metadata: Metadata = {
  title: "Client",
  description: "Client page.",
}

// Simulate a database read for tasks.
async function getTasks() {
  const data = await fs.readFile(
    path.join(process.cwd(), "app/client/data/tasks.json")
  )

  const tasks = JSON.parse(data.toString())

  return z.array(clientSchema).parse(tasks)
}

export default async function TaskPage() {
  const tasks = await getTasks()
  return (
    <div className="h-full flex-1 flex-col space-y-8 py-8">
      <DataTable data={tasks} columns={columns} />
    </div>
  )
}
