import { Injectable } from '@nestjs/common'
import { Cron, CronExpression } from '@nestjs/schedule'

@Injectable()
export class TaskService {
    @Cron(CronExpression.EVERY_MINUTE)
    handleCron() {
        console.log('定时任务执行！')
    }
}
