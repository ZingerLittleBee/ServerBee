import { Module } from '@nestjs/common'
import { TaskService } from './task.service'
import { DbModule } from '@/db/db.module'

@Module({
    imports: [DbModule],
    providers: [TaskService]
})
export class TaskModule {}
