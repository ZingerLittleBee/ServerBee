import { Injectable } from '@nestjs/common'
import { MailerService } from '@nestjs-modules/mailer'

@Injectable()
export class NotificationService {
    constructor(private readonly mailerService: MailerService) {}

    public async example() {
        await this.mailerService.sendMail({
            to: 'xxx@xxx.com',
            subject: 'Validation Code - ServerOctopus',
            template: 'ValidationCode',
            context: {
                validationCode: '123456'
            }
        })
    }
}
