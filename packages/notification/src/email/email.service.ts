import { Injectable } from '@nestjs/common'
import { MailerService } from '@nestjs-modules/mailer'
import { SendValidationCodeDto } from '@server-octopus/types'

@Injectable()
export class EmailService {
    constructor(private readonly mailerService: MailerService) {}

    async sendValidationCode({
        email,
        validationCode,
        subject
    }: SendValidationCodeDto) {
        await this.mailerService.sendMail({
            to: email,
            subject: subject ?? 'Validation Code - ServerOctopus',
            template: 'ValidationCode',
            context: {
                validationCode
            }
        })
    }
}
