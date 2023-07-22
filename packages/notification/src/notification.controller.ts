import { Controller, Get } from '@nestjs/common'
import { NotificationService } from './notification.service'

@Controller()
export class NotificationController {
    constructor(private readonly service: NotificationService) {}

    @Get()
    test() {
        this.service.example()
    }
}
