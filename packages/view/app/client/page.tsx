import { promises as fs } from "fs"
import path from "path"
import { Metadata } from "next"
import { PlusCircle } from "lucide-react"
import { z } from "zod"

import { Button } from "@/components/ui/button"

import { columns } from "./components/columns"
import { DataTable } from "./components/data-table"
import { taskSchema } from "./data/schema"

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

  return z.array(taskSchema).parse(tasks)
}

export default async function TaskPage() {
  const tasks = await getTasks()

  return (
    <div className="h-full flex-1 flex-col space-y-8 py-8">
      <DataTable data={tasks} columns={columns} />
    </div>
  )
}
