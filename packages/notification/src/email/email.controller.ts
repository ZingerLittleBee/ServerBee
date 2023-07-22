import { Controller } from '@nestjs/common'
import { EmailService } from './email.service'
import { EventPattern } from '@nestjs/microservices'
import { kNotificationEmailSend } from '@server-octopus/shared'
import { SendValidationCodeDto } from '@server-octopus/types'

@Controller()
export class EmailController {
    constructor(private readonly emailService: EmailService) {}

    @EventPattern(kNotificationEmailSend)
    async sendValidationCode(data: SendValidationCodeDto) {
        return this.emailService.sendValidationCode(data)
    }
}
