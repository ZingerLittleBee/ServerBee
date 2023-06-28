import { Injectable } from '@nestjs/common'
import { Cron, CronExpression } from '@nestjs/schedule'
import { RedisService } from '@/db/redis.service'

@Injectable()
export class TaskService {
    constructor(private readonly redisService: RedisService) {}

    @Cron(CronExpression.EVERY_MINUTE)
    handleCron() {
        console.log('定时任务执行！')
    }
}
